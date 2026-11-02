"""Reels: an approved portrait becomes a 5-second image-to-video clip."""
from pathlib import Path

from . import higgsfield, state


def run(limit: int = 1) -> dict:
    cal = state.calendar()
    summary = {"made": 0, "errors": []}
    todo = [s for s in cal["slots"] if s["kind"] == "reel" and s["status"] == "ready" and not s.get("video")]
    if not todo:
        return summary
    session = higgsfield.Session()
    for slot in todo[:limit]:
        source = next((a for a in slot["assets"] if a["approved"]), None)
        if not source:
            continue
        try:
            url = higgsfield.upload(Path(source["path"]))
            result = session.image_to_video(url, slot["motion"], duration=5, note=f"reel {slot['id']}")
            dest = state.slot_dir(slot["id"]) / f"{slot['id']}.mp4"
            higgsfield.download(result["video"]["url"], dest)
            slot["video"] = str(dest)
            summary["made"] += 1
            state.log(f"reel: {slot['id']} rendered -> {dest.name}")
        except higgsfield.BudgetExceeded as e:
            summary["errors"].append(str(e))
            break
        except higgsfield.HiggsfieldError as e:
            slot["last_error"] = str(e)
            summary["errors"].append(f"{slot['id']}: {e}")
        finally:
            state.save_calendar(cal)
    summary["spent_usd"] = round(session.spent, 4)
    return summary
