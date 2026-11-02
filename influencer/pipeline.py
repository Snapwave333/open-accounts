"""Nova Vexley pipeline. The OpenAccounts autonomy runtime runs these on the studio's cadences:

  py pipeline.py status              what is configured, approved, due and spent
  py pipeline.py plan                keep the rolling 30-day calendar and the Needs You list current
  py pipeline.py handles             check which handle candidate is free (read-only relay lookups)
  py pipeline.py reference           generate the reference set (needs HIGGSFIELD_API_KEY + spend_authorized)
  py pipeline.py generate [--limit N]  images + QC + captions for the next slots
  py pipeline.py reels               image-to-video for ready reel slots
  py pipeline.py publish [--dry]     post the next due slot (needs account_created + publishing_authorized)
  py pipeline.py analytics           profile snapshot; weekly weight review when due
  py pipeline.py gc [--dry]          delete posted/rejected media from this machine
  py pipeline.py approve GATE [--note ...] / revoke GATE
"""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from oa_influencer import analytics, character, config, generator, higgsfield, planner, publisher, reels, relay, retention, state  # noqa: E402


class Waiting(Exception):
    """Blocked on a human gate or a busy resource: not a failure (exit code 3)."""


def needs_you() -> list[dict]:
    """Every human gate that is still open, in the order it blocks the pipeline."""
    st = state.state()
    pending = []
    if not state.granted("spend_authorized"):
        extra = "" if config.higgsfield_key() else "(no HIGGSFIELD_API_KEY found yet)"
        pending.append(state.needs_you_item("spend_authorized", extra))
    if not state.granted("reference_approved"):
        have = sorted(p.name for p in generator.REFERENCE_DIR.glob("*.jpg")) if generator.REFERENCE_DIR.is_dir() else []
        if have:
            pending.append(state.needs_you_item("reference_approved", f"{len(have)} image(s) ready in {generator.REFERENCE_DIR}"))
    if not state.granted("account_created"):
        handle = st.get("handle")
        if handle:
            pending.append(state.needs_you_item("account_created", f"Handle: @{handle}."))
    if state.granted("account_created") and not state.granted("publishing_authorized"):
        ready = [s["id"] for s in state.calendar()["slots"] if s["status"] == "ready"]
        if ready:
            pending.append(state.needs_you_item("publishing_authorized", f"{len(ready)} post(s) ready: {', '.join(ready[:3])}"))
    state.publish_needs_you(pending)
    return pending


def cmd_status(_a) -> dict:
    st = state.state()
    cal = state.calendar()
    by_status: dict[str, int] = {}
    for s in cal["slots"]:
        by_status[s["status"]] = by_status.get(s["status"], 0) + 1
    out = {
        "character": character.spec()["name"],
        "handle": st.get("handle"),
        "higgsfield_key": bool(config.higgsfield_key()),
        "nvidia_key": bool(config.nvidia_key()),
        "studio_root": str(config.STUDIO_ROOT),
        "approvals": sorted(state.approvals().get("gates", {})),
        "needs_you": [t["id"] for t in needs_you()],
        "calendar": by_status,
        "next_due": (planner.next_due_post(cal) or {}).get("id"),
        "spent_today_usd": state.spent_today_usd(),
        "daily_cap_usd": config.DAILY_USD_CAP,
        "posts": len([p for p in state.posts() if p["result"] == "posted"]),
    }
    return out


def cmd_plan(_a) -> dict:
    out = planner.build()
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


def cmd_reference(_a) -> dict:
    if not state.granted("spend_authorized"):
        needs_you()
        raise Waiting("waiting for approval: spend_authorized (see Needs You)")
    paths = generator.generate_reference(higgsfield.Session())
    needs_you()
    return {"reference": [p.name for p in paths], "approve_with": "py pipeline.py approve reference_approved"}


def cmd_generate(a) -> dict:
    if not state.granted("spend_authorized"):
        needs_you()
        raise Waiting("waiting for approval: spend_authorized (see Needs You)")
    captioned = generator.caption_pending()
    out = generator.run(limit=a.limit)
    out["captioned_late"] = captioned
    needs_you()
    return out


def cmd_reels(_a) -> dict:
    if not state.granted("spend_authorized"):
        raise Waiting("waiting for approval: spend_authorized")
    return reels.run()


def cmd_publish(a) -> dict:
    try:
        return publisher.run(dry=a.dry)
    except publisher.NotAllowed as e:
        needs_you()
        raise Waiting(str(e))


def cmd_analytics(_a) -> dict:
    if not state.granted("account_created"):
        raise Waiting("waiting for approval: account_created")
    out = {"snapshot": analytics.snapshot()}
    if analytics.weekly_due():
        out["weekly"] = analytics.weekly_review()
    return out


def cmd_gc(a) -> dict:
    return retention.run(apply=not a.dry)


def cmd_approve(a) -> dict:
    state.grant(a.gate, by="Eli (CLI)", note=a.note or "")
    return {"granted": a.gate, "needs_you": [t["id"] for t in needs_you()]}


def cmd_revoke(a) -> dict:
    state.revoke(a.gate)
    return {"revoked": a.gate, "needs_you": [t["id"] for t in needs_you()]}


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("status").set_defaults(fn=cmd_status)
    sub.add_parser("plan").set_defaults(fn=cmd_plan)
    sub.add_parser("handles").set_defaults(fn=cmd_handles)
    sub.add_parser("reference").set_defaults(fn=cmd_reference)
    g = sub.add_parser("generate")
    g.add_argument("--limit", type=int, default=2)
    g.set_defaults(fn=cmd_generate)
    sub.add_parser("reels").set_defaults(fn=cmd_reels)
    pub = sub.add_parser("publish")
    pub.add_argument("--dry", action="store_true")
    pub.set_defaults(fn=cmd_publish)
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

    a = p.parse_args(argv)
    try:
        out = a.fn(a)
    except (higgsfield.NotConfigured, higgsfield.BudgetExceeded) as e:
        needs_you()
        print(f"waiting: {e}")
        return 3
    except (relay.RelayUnavailable, Waiting) as e:
        print(f"waiting: {e}")
        return 3
    print(json.dumps(out, ensure_ascii=False, default=str))
    return 0


if __name__ == "__main__":
    sys.exit(main())
