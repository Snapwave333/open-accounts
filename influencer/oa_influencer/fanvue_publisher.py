"""Fanvue Publisher agent: executes the Fanvue calendar against the real account.

Shares Nova's character identity, QC and creative assets with the Instagram publisher,
but runs Fanvue's own calendar (fanvue_strategist), content policy (fanvue_policy) and
monetization. Before a single byte is uploaded the slot passes the compliance gate: banned
text, missing AI disclosure, 18+ markers for Intimate styling, IP-safe cosplay naming,
and per-asset prompt review. A policy block stops the publish - it never "fixes itself"
by silently rewording.

Publishing is real API work: multipart upload per image (create-upload-session -> PUT ->
complete), then create-post with the collected mediaUuids, optional PPV price + preview
and native publishAt scheduling. A post counts as "posted" only after Fanvue's own post
list confirms it; otherwise the entry stays "pending" and the next run verifies.
"""

from __future__ import annotations

import datetime as dt
from pathlib import Path
from typing import Any

from . import config, fanvue_client, fanvue_inventory, fanvue_policy, generator, state
from .fanvue_strategist import FanvueContentStrategist


class NotAllowed(RuntimeError):
    """Raised when a human gate or policy blocks the action."""


MAX_IMAGES_PER_POST = 10  # create-post mediaUuids cap used conservatively


class FanvuePublisher:
    """Publishes ready Fanvue calendar slots to the Fanvue profile."""

    def __init__(self, dry: bool = False) -> None:
        self.dry = dry
        self.client = fanvue_client.FanvueClient()
        self.strategist = FanvueContentStrategist()
        self._creator_uuid: str | None = None

    # ---- gates ----------------------------------------------------------------

    def _gates(self) -> None:
        if not state.granted("account_created"):
            raise NotAllowed("waiting for approval: account_created (Instagram handle chosen)")
        if not state.state().get("handle"):
            raise NotAllowed("no handle chosen yet (run `pipeline.py handles`)")
        # Fanvue publishing is authorized separately from Instagram.
        if not state.granted("fanvue_publishing_authorized"):
            raise NotAllowed("waiting for approval: fanvue_publishing_authorized")

    # ---- ledger ---------------------------------------------------------------

    def _fanvue_posts(self) -> list[dict]:
        return [p for p in state.posts() if p.get("platform") == "fanvue"]

    def _record_post(self, entry: dict) -> None:
        state.record_post(entry)

    # ---- spacing ---------------------------------------------------------------

    def _spacing_ok(self) -> str | None:
        now = state.now()
        recent = [p for p in self._fanvue_posts() if p.get("result") in ("posted", "scheduled")]
        today = [p for p in recent if p["at"][:10] == now.date().isoformat()]
        if len(today) >= config.FANVUE_POSTS_PER_DAY:
            return f"daily cap reached ({config.FANVUE_POSTS_PER_DAY} fanvue posts/day)"
        if recent:
            last = dt.datetime.fromisoformat(max(p["at"] for p in recent))
            gap = now - last
            if gap < dt.timedelta(hours=config.FANVUE_MIN_GAP_HOURS):
                mins = gap.seconds // 60
                return (f"last fanvue post {mins // 60}h{mins % 60}m ago, "
                        f"minimum gap {config.FANVUE_MIN_GAP_HOURS}h")
        return None

    # ---- compliance ------------------------------------------------------------

    def _policy_gate(self, slot: dict, caption: str) -> list[str]:
        """Every caption, message and prompt goes through the Fanvue ruleset first."""
        ok, violations = fanvue_policy.gate_slot(slot, caption)
        if ok and not fanvue_policy.check_caption(caption, fanvue_policy.content_class_for_text(caption)):
            return []
        return violations

    # ---- publish flow ----------------------------------------------------------

    def run(self) -> dict[str, Any]:
        """Publish (or natively schedule) the next due, Fanvue-ready slot."""
        self._gates()

        cal = self.strategist.load_calendar()
        slots = [s for s in cal["slots"] if s["status"] == "ready"]
        slots.sort(key=lambda s: s["scheduled_at"])
        now = state.now()
        slot = next((s for s in slots if dt.datetime.fromisoformat(s["scheduled_at"]) <= now + dt.timedelta(hours=1)),
                    None)
        if not slot:
            return {"result": "nothing due"}

        reason = self._spacing_ok()
        if reason:
            return {"result": "skipped", "slot": slot["id"], "reason": reason}

        approved = [a for a in slot.get("assets", []) if a.get("approved") and Path(a["path"]).is_file()]
        if not approved:
            return {"result": "skipped", "slot": slot["id"], "reason": "no approved image"}

        caption = slot.get("caption")
        if not caption:
            caption = generator.caption_for(slot, Path(approved[0]["path"]))
            slot["caption"] = caption

        violations = self._policy_gate(slot, caption)
        if violations:
            slot["policy_blocked"] = violations[:5]
            self.strategist.save_calendar(cal)
            state.log(f"fanvue publish: {slot['id']} BLOCKED by policy: {violations[0][:160]}")
            return {"result": "blocked_by_policy", "slot": slot["id"], "violations": violations}

        # Duplicate-publication guard: never publish the same slot twice.
        already = [p for p in self._fanvue_posts() if p.get("slot") == slot["id"]]
        if any(p.get("result") in ("posted", "scheduled", "pending") for p in already):
            return {"result": "skipped", "slot": slot["id"], "reason": "already published/queued"}

        media_paths = [Path(a["path"]) for a in approved[:MAX_IMAGES_PER_POST]]
        is_ppv = slot.get("kind") == "ppv"
        audience = "subscribers" if slot.get("tier") == "subscriber" else "followers-and-subscribers"
        price = int(slot["unlock_price_cents"]) if is_ppv else None
        scheduled_at = dt.datetime.fromisoformat(slot["scheduled_at"])
        publish_at = scheduled_at.isoformat() if scheduled_at > now else None

        if self.dry:
            state.log(f"fanvue publish (dry): {slot['id']} -> {len(media_paths)} image(s)"
                      f"{' PPV ' + str(price) + 'c' if price else ''}")
            return {"result": "dry", "slot": slot["id"], "media": str(media_paths[0]),
                    "images": len(media_paths), "price_cents": price}

        if not self.client.available():
            return {"result": "failed", "slot": slot["id"], "reason": "Fanvue MCP not available"}

        try:
            report = self.client.verify()
            self.client.persist_verify_report(report)
            self._creator_uuid = report["creator_uuid"]

            media_uuids = [self.client.upload_image(p) for p in media_paths]
            preview_uuid = media_uuids[0] if is_ppv else None
            created = self.client.create_post_full(
                text=caption,
                media_uuids=media_uuids,
                audience=audience,
                price=price,
                preview_uuid=preview_uuid,
                publish_at=publish_at,
            )
        except fanvue_client.FanvueUnavailable as e:
            state.log(f"fanvue publish: {slot['id']} failed: {e}")
            return {"result": "failed", "slot": slot["id"], "reason": str(e)}
        except Exception as e:  # upload/PUT/API errors: slot stays ready for the next run
            state.log(f"fanvue publish: {slot['id']} failed: {e}")
            slot["last_error"] = str(e)[:300]
            self.strategist.save_calendar(cal)
            return {"result": "failed", "slot": slot["id"], "reason": str(e)[:300]}

        post_uuid = (created or {}).get("uuid") or (created or {}).get("postUuid") or ""
        verified = self._verify_post(post_uuid) if post_uuid else self._verify_by_caption(caption)
        entry = {
            "at": state.iso(), "slot": slot["id"], "kind": slot.get("kind"), "tier": slot.get("tier"),
            "media": str(media_paths[0]), "images": len(media_paths),
            "handle": state.state().get("handle"), "platform": "fanvue",
            "result": "scheduled" if publish_at else ("posted" if verified else "pending"),
            "verified": verified, "post_uuid": post_uuid, "price_cents": price,
            "window": slot.get("window"), "dry": self.dry,
            "fanvue_post": created,
        }
        self._record_post(entry)
        if entry["result"] in ("posted", "scheduled"):
            slot["status"] = "posted" if entry["result"] == "posted" else "scheduled"
            slot["posted_at"] = entry["at"]
            slot["fanvue_post_uuid"] = post_uuid
        else:
            slot["status"] = "pending_verification"
        if slot.get("inventory_ids"):
            fanvue_inventory.mark_posted(slot["inventory_ids"], slot["id"],
                                         status="posted" if entry["result"] == "posted" else "reserved")
        elif entry["result"] == "posted":
            fanvue_inventory.mark_posted([a["id"] for a in approved], slot["id"])
        self.strategist.save_calendar(cal)
        state.log(f"fanvue publish: {slot['id']} -> {entry['result']} ({len(media_paths)} images"
                  f"{', PPV ' + str(price) + 'c' if price else ''})")
        return {"result": entry["result"], "slot": slot["id"], "post_uuid": post_uuid,
                "verified": verified, "price_cents": price}

    def verify_pending(self) -> int:
        """Confirms queued/pending posts by looking them up in Fanvue's own post list.
        Entries are the ledger's own dicts, so mutating + rewriting persists the result."""
        pending = [p for p in self._fanvue_posts() if p.get("result") == "pending"]
        confirmed = 0
        for entry in pending:
            uuid = entry.get("post_uuid") or ""
            ok = (self._verify_post(uuid) if uuid
                  else self._verify_by_caption((entry.get("fanvue_post") or {}).get("text", "")))
            if ok:
                entry["result"] = "posted"
                entry["verified"] = True
                confirmed += 1
                self._mark_slot_posted(entry["slot"], uuid)
        if confirmed:
            state.write_atomic(state.POSTS, state.posts())
            state.log(f"fanvue publish: verified {confirmed} pending post(s)")
        return confirmed

    def _mark_slot_posted(self, slot_id: str, post_uuid: str) -> None:
        cal = self.strategist.load_calendar()
        for s in cal["slots"]:
            if s["id"] == slot_id and s["status"] in ("ready", "pending_verification"):
                s["status"] = "posted"
                s["posted_at"] = state.iso()
                if post_uuid:
                    s["fanvue_post_uuid"] = post_uuid
                if s.get("inventory_ids"):
                    fanvue_inventory.mark_posted(s["inventory_ids"], slot_id, status="posted")
        self.strategist.save_calendar(cal)

    def _verify_post(self, post_uuid: str) -> bool:
        try:
            page = self.client.get_posts_page()
        except fanvue_client.FanvueUnavailable:
            return False
        except Exception:
            return False
        for post in (page or {}).get("data", []) or (page or {}).get("posts", []) or []:
            if (post.get("uuid") or post.get("id")) == post_uuid:
                return True
        return False

    def _verify_by_caption(self, text: str) -> bool:
        if not text:
            return False
        try:
            page = self.client.get_posts_page()
        except Exception:
            return False
        for post in (page or {}).get("data", []) or (page or {}).get("posts", []) or []:
            if (post.get("text") or "").strip()[:80] == text.strip()[:80]:
                return True
        return False
