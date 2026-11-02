"""Garbage collection: posted and rejected media leave this machine; thumbnails and metadata stay."""
import datetime as dt
import shutil
from pathlib import Path

from . import config, state

MEDIA = {".jpg", ".jpeg", ".png", ".mp4", ".webp"}


def _older_than(iso: str | None, hours: int) -> bool:
    if not iso:
        return False
    return state.now() - dt.datetime.fromisoformat(iso) >= dt.timedelta(hours=hours)


def _delete_media(folder: Path) -> int:
    freed = 0
    if not folder.is_dir():
        return 0
    for p in folder.iterdir():
        if p.is_file() and p.suffix.lower() in MEDIA:
            freed += p.stat().st_size
            p.unlink()
    return freed


def run(apply: bool = True) -> dict:
    cal = state.calendar()
    freed, removed = 0, []
    for slot in cal["slots"]:
        folder = state.slot_dir(slot["id"])
        posted = slot["status"] == "posted" and _older_than(slot.get("posted_at"), config.KEEP_POSTED_HOURS)
        stale = slot["status"] == "rejected" and slot["attempts"] >= config.MAX_GENERATION_ATTEMPTS and _older_than(slot.get("scheduled_at"), config.KEEP_REJECTED_HOURS)
        expired = slot["status"] in ("planned", "rejected") and _older_than(slot.get("scheduled_at"), 24) and folder.is_dir()
        if not (posted or stale or expired):
            continue
        if not slot.get("gc_at"):
            size = _delete_media(folder) if apply else sum(p.stat().st_size for p in folder.glob("*") if p.suffix.lower() in MEDIA)
            if size:
                freed += size
                removed.append(slot["id"])
            if apply:
                slot["gc_at"] = state.iso()
                if expired and slot["status"] != "posted":
                    slot["status"] = "expired"
                for a in slot.get("assets", []):
                    a["path"] = None
                if slot.get("video"):
                    slot["video"] = None
    if apply:
        state.save_calendar(cal)
    # Higgsfield keeps outputs 7 days on its side; nothing to do there. Logs: keep the last 2 MB.
    log = state.LOG
    if log.exists() and log.stat().st_size > 2_000_000:
        text = log.read_text(encoding="utf8")[-1_000_000:]
        log.write_text(text, encoding="utf8")
    free_gb = shutil.disk_usage(config.DATA if config.DATA.exists() else Path.cwd()).free / 1e9
    summary = {"applied": apply, "freed_mb": round(freed / 1e6, 1), "slots": removed, "free_gb": round(free_gb, 1)}
    state.log(f"gc: {'freed' if apply else 'would free'} {summary['freed_mb']} MB in {len(removed)} slot(s); {summary['free_gb']} GB free")
    return summary
