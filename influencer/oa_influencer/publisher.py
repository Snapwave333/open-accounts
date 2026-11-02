"""Social Media Manager: posts the next due, approved slot on Nova's account. Refuses without the
human gates, respects the daily cap and spacing, and records exactly what Instagram confirmed."""
import datetime as dt
from pathlib import Path

from . import config, planner, relay, state


class NotAllowed(RuntimeError):
    pass


def _gates() -> None:
    for gate in ("account_created", "publishing_authorized"):
        if not state.granted(gate):
            raise NotAllowed(f"waiting for approval: {gate}")
    if not state.state().get("handle"):
        raise NotAllowed("no handle chosen yet (run `pipeline.py handles`)")


def _spacing_ok(kind: str) -> str | None:
    now = state.now()
    recent = [p for p in state.posts() if p["result"] == "posted" and p["kind"] != "reel"]
    today = [p for p in recent if p["at"][:10] == now.date().isoformat()]
    if kind != "reel" and len(today) >= config.FEED_POSTS_PER_DAY:
        return f"daily cap reached ({config.FEED_POSTS_PER_DAY} feed post/day)"
    if recent:
        last = dt.datetime.fromisoformat(max(p["at"] for p in recent))
        gap = now - last
        if kind != "reel" and gap < dt.timedelta(hours=config.MIN_FEED_GAP_HOURS):
            return f"last feed post {gap.seconds // 3600}h ago, minimum gap {config.MIN_FEED_GAP_HOURS}h"
    return None


def run(dry: bool = False) -> dict:
    _gates()
    cal = state.calendar()
    slot = planner.next_due_post(cal)
    if not slot:
        return {"result": "nothing due"}
    reason = _spacing_ok(slot["kind"])
    if reason:
        return {"result": "skipped", "slot": slot["id"], "reason": reason}

    handle = state.state()["handle"]
    if slot["kind"] == "reel":
        if not slot.get("video"):
            return {"result": "skipped", "slot": slot["id"], "reason": "reel not rendered yet"}
        media = Path(slot["video"])
    else:
        first = next((a for a in slot["assets"] if a["approved"]), None)
        if not first:
            return {"result": "skipped", "slot": slot["id"], "reason": "no approved image"}
        media = Path(first["path"])
        if slot["kind"] == "carousel":
            # The relay's share flow uploads one file; carousels publish their lead frame.
            state.log(f"publish: {slot['id']} is a carousel; posting the lead frame (relay uploads one file)")
    if not media.exists():
        slot["status"] = "rejected"
        slot["last_error"] = f"media missing: {media}"
        state.save_calendar(cal)
        return {"result": "failed", "slot": slot["id"], "reason": slot["last_error"]}

    with relay.studio_session(handle) as (_tr, ig):
        outcome = ig.share_media(media, slot["caption"], slot["id"], dry=dry, ai_label=True)

    entry = {"at": state.iso(), "slot": slot["id"], "kind": slot["kind"], "media": str(media),
             "handle": handle, "result": outcome, "dry": dry}
    state.record_post(entry)
    if outcome == "posted" and not dry:
        slot["status"] = "posted"
        slot["posted_at"] = entry["at"]
    elif outcome == "wrong-account":
        slot["last_error"] = f"@{handle} is not signed in on this Chrome"
        state.revoke("account_created")
    elif outcome not in ("dry", "pending"):
        slot["last_error"] = f"share returned {outcome}"
    state.save_calendar(cal)
    state.log(f"publish: {slot['id']} -> {outcome}{' (dry)' if dry else ''}")
    return {"result": outcome, "slot": slot["id"], "handle": handle}
