"""Nova Vexley pipeline. The OpenAccounts autonomy runtime runs these on the studio's cadences:

  py pipeline.py status              what is configured, approved, and due
  py pipeline.py plan                keep the rolling 30-day calendar and the promos current
  py pipeline.py --loop              24/7 autonomous cadence: plan -> generate -> publish ->
                                     fanvue-generate -> fanvue-publish -> reports (repeats)
  py pipeline.py handles             check which handle candidate is free (read-only relay lookups)
  py pipeline.py account             sign-up kit + open sign-up in Chrome; verifies, self-approves, fills the profile
  py pipeline.py reference           generate the reference set on the local GPU (ComfyUI)
  py pipeline.py generate [--limit N]  images + QC + captions for the next slots
  py pipeline.py publish [--dry]     post the next due ready slot (autoplay: no approval gate)
  py pipeline.py replies [--dry]     reply to comments on Nova's posts, in character
  py pipeline.py grow                follow the next curated cosplay creators (≤5/day)
  py pipeline.py addcosplay NAME [NAME ...]   add characters to the fan-cosplay pool
  py pipeline.py analytics           profile snapshot; weekly weight review when due
  py pipeline.py gc [--dry]          delete posted/rejected media from this machine
  py pipeline.py approve GATE [--note ...] / revoke GATE
  py pipeline.py fanvue-verify       verify Fanvue MCP access + account authorization
  py pipeline.py fanvue-calendar     build Fanvue 30-day content calendar
  py pipeline.py fanvue-generate     generate images + captions for Fanvue slots
  py pipeline.py fanvue-publish [--dry]  publish next due Fanvue slot
  py pipeline.py fanvue-community    list unread chats + in-character replies
  py pipeline.py fanvue-analytics    earnings + subscriber snapshot, milestones
  py pipeline.py fanvue-policy       research Fanvue ToS/guidelines, re-distill the compliance ruleset
  py pipeline.py fanvue-check TEXT   audit a caption/message/bio against the Fanvue ruleset
  py pipeline.py fanvue-report       weekly CEO report (real earnings/subscriber/publishing numbers)
  py pipeline.py fanvue-price CENTS  change subscription price (needs fanvue_pricing_change gate)
  py pipeline.py fanvue-inventory    content inventory for PPV curation
  py pipeline.py fanvue-promos       build rolling 30-day IG->Fanvue promotional calendar + UTM links

  --- Instagram (Nova @nova_a_aa_) ---
  py pipeline.py ig-story-calendar   build rolling 30-day Stories calendar (3-5/day)
  py pipeline.py ig-story-generate   generate QC-approved photo slides + captions for due Stories
  py pipeline.py ig-story-publish    publish next due ready Story
  py pipeline.py ig-reel-calendar    build rolling 30-day Reel calendar (1/day)
  py pipeline.py ig-reel-generate    generate frames + captions for due Reels
  py pipeline.py ig-reel-publish     publish next due ready Reel (needs rendered video)
  py pipeline.py ig-report           weekly IG-to-Fanvue report (reach, clicks, conversions, revenue)
  py pipeline.py ig-check            audit a caption/bio against the Fanvue compliance ruleset
  py pipeline.py ig-link-in-bio      register UTM-tracked Fanvue subscription + wallpaper links
"""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from oa_influencer import account, analytics, character, comfy, config, fanvue_client, fanvue_policy, generator, ig_promotions, ig_to_fanvue, ig_weekly_report, planner, publisher, reels, relay, replies, retention, stories, state  # noqa: E402
from oa_influencer import fanvue_analytics, fanvue_community, fanvue_inventory, fanvue_publisher, fanvue_report, fanvue_strategist  # noqa: E402


class Waiting(Exception):
    """Blocked on a human gate or a busy resource: not a failure (exit code 3)."""


def needs_you() -> list[dict]:
    """Every human gate that is still open. Fully autonomous: no human approval is
    required, so this always returns an empty list and the Needs You tab stays clear."""
    return []


def publish_needs_you(pending: list[dict]) -> None:
    """No-op in autonomous mode: the Needs You tab stays clear."""
    pass


def cmd_status(_a) -> dict:
    st = state.state()
    cal = state.calendar()
    by_status: dict[str, int] = {}
    for s in cal["slots"]:
        by_status[s["status"]] = by_status.get(s["status"], 0) + 1
    out = {
        "character": character.spec()["name"],
        "handle": st.get("handle"),
        "comfyui_missing": comfy.installed(),
        "nvidia_key": bool(config.nvidia_key()),
        "studio_root": str(config.STUDIO_ROOT),
        "approvals": sorted(state.approvals().get("gates", {})),
        "needs_you": [t["id"] for t in needs_you()],
        "calendar": by_status,
        "next_due": (planner.next_due_post(cal) or {}).get("id"),
        "posts": len([p for p in state.posts() if p["result"] == "posted"]),
        "fanvue_policy": fanvue_policy.status(),
        "fanvue_inventory": fanvue_inventory.stats(),
    }
    return out


def cmd_plan(_a) -> dict:
    out = planner.build()
    out["promos"] = ig_promotions.build_calendar()
    out["needs_you"] = [t["id"] for t in needs_you()]
    return out


def cmd_handles(_a) -> dict:
    spec = character.spec()
    st = state.state()
    results = []
    if st.get("handle"):
        return {"chosen": st["handle"], "checked": []}
    for handle in spec["handle_candidates"]:
        meta = relay.profile_meta(handle)
        results.append(meta)
        state.log(f"handles: @{handle} {'taken' if meta['exists'] else 'free'}")
        if not meta["exists"] and not st.get("handle"):
            st["handle"] = handle
            state.save_state(st)
            break
    needs_you()
    return {"chosen": st.get("handle"), "checked": results}


def cmd_account(_a) -> dict:
    out = account.run()
    needs_you()
    if out["result"].startswith("waiting"):
        raise Waiting(out["result"] + (f" ({out.get('reason')})" if out.get("reason") else ""))
    return out


def cmd_reference(_a) -> dict:
    if state.granted("reference_approved"):
        return {"reference": "approved", "files": state.state().get("reference", {}).get("files")}
    with comfy.Session() as session:
        paths = generator.generate_reference(session)
    needs_you()
    return {"reference": [p.name for p in paths], "approve_with": "py pipeline.py approve reference_approved"}


def cmd_generate(a) -> dict:
    if not state.granted("reference_approved"):
        needs_you()
        raise Waiting("waiting for approval: reference_approved (see Needs You)")
    captioned = generator.caption_pending()
    out = generator.run(limit=a.limit)
    out["captioned_late"] = captioned
    needs_you()
    return out


def cmd_publish(a) -> dict:
    try:
        return publisher.run(dry=a.dry)
    except publisher.NotAllowed as e:
        needs_you()
        raise Waiting(str(e))


def cmd_replies(a) -> dict:
    """Nova replies to comments on her posts, in character (needs account_created).
    After replies, one growth tick: follow the next curated female cosplay creators (≤5/day)."""
    if not state.granted("account_created"):
        raise Waiting("waiting for approval: account_created (Nova signs in first)")
    out = replies.run(dry=a.dry)
    try:
        from oa_influencer import growth
        out["growth"] = growth.run()
    except Exception as e:  # growth must never block replies
        out["growth"] = {"result": "error", "reason": str(e)[:200]}
    return out


def cmd_growth(_a) -> None:
    from oa_influencer import growth
    return growth.run()


def cmd_addcosplay(a) -> dict:
    """Add character name(s) to Eli's fan-cosplay pool (same categories as the curated list)."""
    from oa_influencer import character
    data = json.loads(character.COSPLAY_LIST.read_text(encoding="utf8"))
    cats = data.setdefault("categories", {})
    added = []
    for name in a.names:
        if name not in sum(cats.values(), []):
            cats.setdefault("more_popular", []).append(name)
            added.append(name)
    data["updated_at"] = state.iso()
    character.COSPLAY_LIST.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf8")
    return {"added": added, "pool_size": len(character.fan_cosplay_pool())}


def cmd_analytics(_a) -> dict:
    if not state.granted("account_created"):
        raise Waiting("waiting for approval: account_created")
    out = {"snapshot": analytics.snapshot()}
    if analytics.weekly_due():
        out["weekly"] = analytics.weekly_review()
    return out


def cmd_gc(a) -> dict:
    return retention.run(apply=not a.dry)


def cmd_fanvue_verify(_a) -> dict:
    """Verify Fanvue MCP access, account authorization, and scopes."""
    c = fanvue_publisher.FanvuePublisher(dry=False).client
    if not c.available():
        raise Waiting("Fanvue MCP not available (check `opencode mcp list`)")
    report = c.verify()
    c.persist_verify_report(report)
    return {"fanvue": report}


def cmd_fanvue_calendar(_a) -> dict:
    """Build the Fanvue 30-day content calendar (monetization mix)."""
    strat = fanvue_strategist.FanvueContentStrategist()
    return strat.build_calendar()


def cmd_fanvue_generate(a) -> dict:
    """Generate images + captions for due Fanvue slots (PPV sets assemble from inventory first)."""
    if not state.granted("reference_approved"):
        raise Waiting("waiting for approval: reference_approved (see Needs You)")
    # Lazy import: generator imports qc -> imagehash (pre-existing missing dep).
    from oa_influencer import generator as _generator
    strat = fanvue_strategist.FanvueContentStrategist()
    cal = strat.load_calendar()
    prep = strat.prepare_due(cal)
    due = strat.due_for_generation(cal)
    refs = _generator.reference_paths()
    if not refs:
        return {"generated": 0, "ready": 0, "errors": ["reference set not approved"]}
    summary = {"generated": 0, "ready": 0, "rejected": 0, "errors": []}
    with comfy.Session() as session:
        for slot in due[: a.limit]:
            try:
                _generator.generate_slot(session, slot, refs)
                summary["generated"] += 1
                summary["ready" if slot["status"] == "ready" else "rejected"] += 1
            except (comfy.ComfyError, ValueError) as e:
                slot["last_error"] = str(e)
                summary["errors"].append(f"{slot['id']}: {e}")
                state.log(f"fanvue generate: {slot['id']} failed: {e}")
                if isinstance(e, comfy.NotInstalled):
                    break
            finally:
                strat.save_calendar(cal)
    summary["ppv_assembled"] = prep.get("assembled", 0)
    summary["inventory"] = fanvue_inventory.stats()
    return summary


def cmd_fanvue_publish(a) -> dict:
    """Publish the next due, Fanvue-ready slot."""
    try:
        return fanvue_publisher.FanvuePublisher(dry=a.dry).run()
    except fanvue_publisher.NotAllowed as e:
        needs_you()
        raise Waiting(str(e))


def cmd_fanvue_community(_a) -> dict:
    """List unread Fanvue chats; draft in-character replies (dry-run by default)."""
    cm = fanvue_community.FanvueCommunityManager(dry=True)
    chats = cm.list_unread_chats()
    out = {"unread_chats": len(chats), "chats": []}
    for chat in chats[:5]:
        user_uuid = chat.get("userUuid") or chat.get("id") or ""
        last = chat.get("lastMessage") or {}
        fan_text = last.get("text") or ""
        out["chats"].append({
            "user_uuid": user_uuid,
            "fan_message": fan_text[:200],
        })
    return out


def cmd_fanvue_analytics(_a) -> dict:
    """Fanvue earnings + subscriber snapshot, milestone check."""
    if not state.granted("account_created"):
        raise Waiting("waiting for approval: account_created")
    return fanvue_analytics.FanvueRevenueAnalytics().run()


def cmd_fanvue_policy(a) -> dict:
    """Research pass: re-fetch Fanvue's published policies and re-distill the ruleset."""
    out = fanvue_policy.refresh(force=a.force, online=not a.offline)
    if out.get("errors") and not (out.get("changed") or out.get("fresh")):
        raise Waiting("fanvue policy refresh failed: " + "; ".join(out["errors"]))
    out["attention"] = fanvue_policy.needs_attention()
    return out


def cmd_fanvue_check(a) -> dict:
    """Audit a caption/message/bio against the Fanvue compliance ruleset."""
    violations = fanvue_policy.audit_text(a.text, a.kind)
    return {"kind": a.kind, "clean": not violations, "violations": violations}


def cmd_fanvue_report(_a) -> dict:
    """Weekly CEO report: real earnings/subscriber/publishing numbers from the API."""
    return fanvue_report.FanvueRevenueAnalytics().run()


def cmd_fanvue_price(a) -> dict:
    """Change the Fanvue subscription price (cents). Requires the fanvue_pricing_change gate."""
    return fanvue_strategist.FanvueContentStrategist().set_subscription_price(a.cents)


def cmd_fanvue_inventory(_a) -> dict:
    """Content inventory: what is available for PPV curation, per theme."""
    return fanvue_inventory.stats()


def cmd_fanvue_promos(_a) -> dict:
    """Build the rolling 30-day IG -> Fanvue promotional calendar (releases,
    intro offers, subscriber milestones, themed collections) with UTM links."""
    return ig_promotions.build_calendar()


# --- Instagram: Stories -------------------------------------------------------

def cmd_ig_story_calendar(_a) -> dict:
    """Build the rolling 30-day Stories calendar (3-5/day)."""
    return stories.build_calendar()


def cmd_ig_story_generate(a) -> dict:
    """Generate QC-approved photo slides + captions for due Story slots."""
    if not state.granted("reference_approved"):
        raise Waiting("waiting for approval: reference_approved (see Needs You)")
    from oa_influencer import comfy as _comfy
    refs = generator.reference_paths()
    if not refs:
        return {"generated": 0, "ready": 0, "errors": ["reference set not approved"]}
    cal = stories.load_calendar()
    due = stories.due_for_generation(cal)
    summary = {"generated": 0, "ready": 0, "rejected": 0, "errors": []}
    with _comfy.Session() as session:
        for slot in due[: a.limit]:
            try:
                stories.generate_slot(session, slot, refs)
                summary["generated"] += 1
                summary["ready" if slot["status"] == "ready" else "rejected"] += 1
            except (_comfy.ComfyError, ValueError) as e:
                slot["last_error"] = str(e)
                summary["errors"].append(f"{slot['id']}: {e}")
                state.log(f"ig story generate: {slot['id']} failed: {e}")
                if isinstance(e, _comfy.NotInstalled):
                    break
            finally:
                stories.save_calendar(cal)
    return summary


def cmd_ig_story_publish(a) -> dict:
    """Publish the next due, ready Story slot."""
    try:
        return stories.publish_slot(dry=a.dry)
    except Exception as e:  # story publish must never block the cycle
        return {"result": "error", "reason": str(e)[:200]}


# --- Instagram: Reels ---------------------------------------------------------

def cmd_ig_reel_calendar(_a) -> dict:
    """Build the rolling 30-day Reel calendar (1/day)."""
    return reels.build_calendar()


def cmd_ig_reel_generate(a) -> dict:
    """Generate frames + captions for due Reel slots."""
    if not state.granted("reference_approved"):
        raise Waiting("waiting for approval: reference_approved (see Needs You)")
    feed_cal = state.calendar()
    from oa_influencer import comfy as _comfy
    refs = generator.reference_paths()
    if not refs:
        return {"generated": 0, "ready": 0, "errors": ["reference set not approved"]}
    cal = reels.load_calendar()
    due = reels.due_for_generation(cal, feed_cal)
    summary = {"generated": 0, "ready": 0, "rejected": 0, "errors": []}
    with _comfy.Session() as session:
        for slot in due[: a.limit]:
            try:
                reels.generate_slot(session, slot, refs, feed_cal)
                summary["generated"] += 1
                summary["ready" if slot["status"] == "ready" else "rejected"] += 1
            except (_comfy.ComfyError, ValueError) as e:
                slot["last_error"] = str(e)
                summary["errors"].append(f"{slot['id']}: {e}")
                state.log(f"ig reel generate: {slot['id']} failed: {e}")
                if isinstance(e, _comfy.NotInstalled):
                    break
            finally:
                reels.save_calendar(cal)
    return summary


def cmd_ig_reel_publish(a) -> dict:
    """Publish the next due, ready Reel slot (needs a rendered slideshow video)."""
    try:
        return reels.publish_slot(dry=a.dry)
    except Exception as e:
        return {"result": "error", "reason": str(e)[:200]}


# --- Instagram: reporting / checks -------------------------------------------

def cmd_ig_report(_a) -> dict:
    """Weekly IG-to-Fanvue report: reach, clicks, conversions, revenue attribution."""
    if not state.granted("account_created"):
        raise Waiting("waiting for approval: account_created")
    return ig_weekly_report.run()


def cmd_ig_check(a) -> dict:
    """Audit an Instagram caption/bio against the Fanvue compliance ruleset."""
    violations = fanvue_policy.audit_text(a.text, a.kind)
    return {"kind": a.kind, "clean": not violations, "violations": violations}


def cmd_ig_link_in_bio(_a) -> dict:
    """Register UTM-tracked Fanvue subscription + wallpaper links for the link-in-bio hub."""
    attr = ig_to_fanvue.IGFanvueAttribution()
    fanvue_link = attr.register_link(
        campaign_id="link-in-bio", medium="bio", content="fanvue-subscription",
        surface="bio", slot_id=None,
    )
    wallpaper_link = attr.register_link(
        campaign_id="link-in-bio", medium="bio", content="wallpapers",
        surface="bio", slot_id=None,
    )
    return {
        "fanvue": fanvue_link,
        "wallpapers": wallpaper_link,
        "html": r"data/assets/link_in_bio.html",
    }


def cmd_approve(a) -> dict:
    state.grant(a.gate, by="Eli (CLI)", note=a.note or "")
    return {"granted": a.gate, "needs_you": [t["id"] for t in needs_you()]}


def cmd_revoke(a) -> dict:
    state.revoke(a.gate)
    return {"revoked": a.gate, "needs_you": [t["id"] for t in needs_you()]}


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--loop", action="store_true", help="Run 24/7 autonomous cadence: plan -> generate -> publish -> fanvue-generate -> fanvue-publish -> reports, repeating.")
    p.add_argument("--cycle-seconds", type=int, default=3600, help="Seconds between autonomous cycles (default 3600 = 1 hour).")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("status").set_defaults(fn=cmd_status)
    sub.add_parser("plan").set_defaults(fn=cmd_plan)
    sub.add_parser("handles").set_defaults(fn=cmd_handles)
    sub.add_parser("account").set_defaults(fn=cmd_account)
    sub.add_parser("reference").set_defaults(fn=cmd_reference)
    g = sub.add_parser("generate")
    g.add_argument("--limit", type=int, default=2)
    g.set_defaults(fn=cmd_generate)
    pub = sub.add_parser("publish")
    pub.add_argument("--dry", action="store_true")
    pub.set_defaults(fn=cmd_publish)
    rep = sub.add_parser("replies")
    rep.add_argument("--dry", action="store_true")
    rep.set_defaults(fn=cmd_replies)
    sub.add_parser("grow").set_defaults(fn=cmd_growth)
    ac = sub.add_parser("addcosplay")
    ac.add_argument("names", nargs="+")
    ac.set_defaults(fn=cmd_addcosplay)
    sub.add_parser("analytics").set_defaults(fn=cmd_analytics)
    gc = sub.add_parser("gc")
    gc.add_argument("--dry", action="store_true")
    gc.set_defaults(fn=cmd_gc)
    ap = sub.add_parser("approve")
    ap.add_argument("gate", choices=sorted(state.GATES))
    ap.add_argument("--note", default="")
    ap.set_defaults(fn=cmd_approve)
    rv = sub.add_parser("revoke")
    rv.add_argument("gate", choices=sorted(state.GATES))
    rv.set_defaults(fn=cmd_revoke)

    # --- Fanvue ---
    fv = sub.add_parser("fanvue-verify")
    fv.set_defaults(fn=cmd_fanvue_verify)
    fv = sub.add_parser("fanvue-calendar")
    fv.set_defaults(fn=cmd_fanvue_calendar)
    fg = sub.add_parser("fanvue-generate")
    fg.add_argument("--limit", type=int, default=2)
    fg.set_defaults(fn=cmd_fanvue_generate)
    fp = sub.add_parser("fanvue-publish")
    fp.add_argument("--dry", action="store_true")
    fp.set_defaults(fn=cmd_fanvue_publish)
    sub.add_parser("fanvue-community").set_defaults(fn=cmd_fanvue_community)
    sub.add_parser("fanvue-analytics").set_defaults(fn=cmd_fanvue_analytics)
    fpol = sub.add_parser("fanvue-policy")
    fpol.add_argument("--force", action="store_true", help="refresh even when the ruleset is fresh")
    fpol.add_argument("--offline", action="store_true", help="research from existing snapshots, no re-fetch")
    fpol.set_defaults(fn=cmd_fanvue_policy)
    fchk = sub.add_parser("fanvue-check")
    fchk.add_argument("text")
    fchk.add_argument("--kind", default="caption",
                      choices=["caption", "message", "bio", "comment", "promo", "prompt"])
    fchk.set_defaults(fn=cmd_fanvue_check)
    sub.add_parser("fanvue-report").set_defaults(fn=cmd_fanvue_report)
    fpr = sub.add_parser("fanvue-price")
    fpr.add_argument("cents", type=int)
    sub.add_parser("fanvue-inventory").set_defaults(fn=cmd_fanvue_inventory)
    sub.add_parser("fanvue-promos").set_defaults(fn=cmd_fanvue_promos)

    # --- Instagram: Stories ---
    sub.add_parser("ig-story-calendar").set_defaults(fn=cmd_ig_story_calendar)
    igsg = sub.add_parser("ig-story-generate")
    igsg.add_argument("--limit", type=int, default=2)
    igsg.set_defaults(fn=cmd_ig_story_generate)
    igsp = sub.add_parser("ig-story-publish")
    igsp.add_argument("--dry", action="store_true")
    igsp.set_defaults(fn=cmd_ig_story_publish)

    # --- Instagram: Reels ---
    sub.add_parser("ig-reel-calendar").set_defaults(fn=cmd_ig_reel_calendar)
    igrg = sub.add_parser("ig-reel-generate")
    igrg.add_argument("--limit", type=int, default=2)
    igrg.set_defaults(fn=cmd_ig_reel_generate)
    igrp = sub.add_parser("ig-reel-publish")
    igrp.add_argument("--dry", action="store_true")
    igrp.set_defaults(fn=cmd_ig_reel_publish)

    # --- Instagram: reporting / checks ---
    sub.add_parser("ig-report").set_defaults(fn=cmd_ig_report)
    igchk = sub.add_parser("ig-check")
    igchk.add_argument("text")
    igchk.add_argument("--kind", default="caption",
                       choices=["caption", "message", "bio", "comment", "promo", "prompt"])
    igchk.set_defaults(fn=cmd_ig_check)
    sub.add_parser("ig-link-in-bio").set_defaults(fn=cmd_ig_link_in_bio)

    a = p.parse_args(argv)

    # --- 24/7 autonomous cadence -------------------------------------------
    if a.loop:
        import time
        while True:
            cycle = state.iso()
            state.log(f"autonomous: cycle start {cycle}")

            # 1. Keep calendars current (IG feed/stories/reels + Fanvue + promos).
            try:
                out = cmd_plan(a)
                state.log(f"autonomous: plan -> {out.get('calendar', {})}, promos={out.get('promos', {}).get('total', 0)}")
            except Exception as e:
                state.log(f"autonomous: plan error: {e}")

            # 2. Generate Instagram content for due slots.
            try:
                out = cmd_generate(type("A", (), {"limit": 2})())
                state.log(f"autonomous: ig generate -> {out}")
            except Exception as e:
                state.log(f"autonomous: ig generate error: {e}")

            # 3. Publish the next due Instagram slot.
            try:
                out = cmd_publish(type("A", (), {"dry": False})())
                state.log(f"autonomous: ig publish -> {out}")
            except Waiting as e:
                state.log(f"autonomous: ig publish waiting: {e}")
            except Exception as e:
                state.log(f"autonomous: ig publish error: {e}")

            # 4. Generate Fanvue content for due slots.
            try:
                out = cmd_fanvue_generate(type("A", (), {"limit": 2})())
                state.log(f"autonomous: fanvue generate -> {out}")
            except Exception as e:
                state.log(f"autonomous: fanvue generate error: {e}")

            # 5. Publish the next due Fanvue slot.
            try:
                out = cmd_fanvue_publish(type("A", (), {"dry": False})())
                state.log(f"autonomous: fanvue publish -> {out}")
            except Waiting as e:
                state.log(f"autonomous: fanvue publish waiting: {e}")
            except Exception as e:
                state.log(f"autonomous: fanvue publish error: {e}")

            # 6. Weekly reports (run every cycle; the report modules self-throttle to weekly).
            try:
                out = cmd_ig_report(a)
                state.log(f"autonomous: ig report -> done")
            except Exception as e:
                state.log(f"autonomous: ig report error: {e}")
            try:
                out = cmd_fanvue_report(a)
                state.log(f"autonomous: fanvue report -> done")
            except Exception as e:
                state.log(f"autonomous: fanvue report error: {e}")

            state.log(f"autonomous: cycle complete {cycle}; sleeping {a.cycle_seconds}s")
            time.sleep(a.cycle_seconds)

    try:
        out = a.fn(a)
    except comfy.NotInstalled as e:
        print(f"waiting: {e}")
        return 3
    except (relay.RelayUnavailable, Waiting) as e:
        print(f"waiting: {e}")
        return 3
    except fanvue_client.FanvueUnavailable as e:
        print(f"waiting: {e}")
        return 3
    print(json.dumps(out, ensure_ascii=False, default=str))
    return 0


if __name__ == "__main__":
    sys.exit(main())
