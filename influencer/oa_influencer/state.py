"""Persistent state on disk: state, approvals, posts, spend ledger, log. All writes are atomic."""
import datetime as dt
import json
import os
from pathlib import Path
from typing import Any

from . import config

STATE = config.DATA / "state.json"
APPROVALS = config.DATA / "approvals.json"
NEEDS_YOU = config.DATA / "needs_you.json"
POSTS = config.DATA / "posts.json"
LEDGER = config.DATA / "ledger.json"
CALENDAR = config.DATA / "calendar.json"
ANALYTICS = config.DATA / "analytics.json"
HASHES = config.DATA / "hashes.json"
LOG = config.DATA / "logs" / "pipeline.log"

GATES = {
    "spend_authorized": {
        "title": "Authorize Higgsfield spending for Nova",
        "instructions": "Add a Higgsfield platform key as the user environment variable HIGGSFIELD_API_KEY "
        "(format id:secret, from console.higgsfield.ai) and fund the account. The pipeline estimates "
        f"every request and stops at ${config.DAILY_USD_CAP:.2f}/day. Approve to let it spend.",
        "page_url": "https://console.higgsfield.ai",
    },
    "reference_approved": {
        "title": "Approve Nova's reference set",
        "instructions": "Look at data/reference/ (profile.jpg and ref-*.jpg). Approve when the face, hair "
        "and marks match character/identity.md; every later image is checked against these.",
        "page_url": None,
    },
    "account_created": {
        "title": "Create Nova's Instagram account",
        "instructions": "Create the account in your own Chrome with the handle in data/state.json "
        "(see brand/instagram.md for name, bio and profile image), stay signed in, then approve. "
        "The relay posts through Instagram's profile switcher.",
        "page_url": "https://www.instagram.com/accounts/emailsignup/",
    },
    "publishing_authorized": {
        "title": "Authorize Nova's first publish",
        "instructions": "Review the ready posts in data/calendar.json (status ready) and their images. "
        "Approve to let the publisher post one feed item per day on the approved account.",
        "page_url": None,
    },
}


def now() -> dt.datetime:
    return dt.datetime.now().astimezone()


def iso(t: dt.datetime | None = None) -> str:
    return (t or now()).isoformat(timespec="seconds")


def write_atomic(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + f".tmp-{os.getpid()}")
    tmp.write_text(json.dumps(data, indent=1, ensure_ascii=False), encoding="utf8")
    os.replace(tmp, path)


def read(path: Path, default: Any) -> Any:
    if not path.exists():
        return default
    return json.loads(path.read_text(encoding="utf8"))


def log(msg: str) -> None:
    line = f"{now().strftime('%Y-%m-%d %H:%M:%S')} {msg}"
    print(line, flush=True)
    LOG.parent.mkdir(parents=True, exist_ok=True)
    with LOG.open("a", encoding="utf8") as f:
        f.write(line + "\n")


# ---- state -----------------------------------------------------------------

def state() -> dict:
    return read(STATE, {"handle": None, "reference": None, "pillar_weights": None})


def save_state(s: dict) -> None:
    write_atomic(STATE, s)


# ---- approvals (human gates) -------------------------------------------------

def approvals() -> dict:
    return read(APPROVALS, {"gates": {}})


def granted(gate: str) -> bool:
    return gate in approvals().get("gates", {})


def grant(gate: str, by: str, note: str = "") -> None:
    if gate not in GATES:
        raise KeyError(f"Unknown gate {gate!r}; known: {', '.join(GATES)}")
    a = approvals()
    a.setdefault("gates", {})[gate] = {"granted_at": iso(), "by": by, "note": note}
    write_atomic(APPROVALS, a)
    log(f"approval: {gate} granted by {by}")


def revoke(gate: str) -> None:
    a = approvals()
    a.get("gates", {}).pop(gate, None)
    write_atomic(APPROVALS, a)
    log(f"approval: {gate} revoked")


def publish_needs_you(pending: list[dict]) -> None:
    """The list the OpenAccounts app shows in its Needs You tab. Each item: id, title,
    instructions, page_url, created_at. Resolving one in the app grants the gate."""
    write_atomic(NEEDS_YOU, {"updated_at": iso(), "tasks": pending})


def needs_you_item(gate: str, extra: str = "") -> dict:
    g = GATES[gate]
    return {
        "id": f"influencer:{gate}",
        "gate": gate,
        "title": g["title"],
        "instructions": (g["instructions"] + (" " + extra if extra else "")).strip(),
        "page_url": g["page_url"],
        "created_at": iso(),
    }


# ---- posts and spend ---------------------------------------------------------

def posts() -> list[dict]:
    return read(POSTS, [])


def record_post(entry: dict) -> None:
    p = posts()
    p.append(entry)
    write_atomic(POSTS, p)


def ledger() -> dict:
    return read(LEDGER, {"entries": []})


def spent_today_usd() -> float:
    today = now().date().isoformat()
    return round(sum(e["usd"] for e in ledger()["entries"] if e["at"].startswith(today)), 4)


def record_spend(model: str, usd: float, request_id: str, note: str = "") -> None:
    l = ledger()
    l["entries"].append({"at": iso(), "model": model, "usd": round(usd, 4), "request_id": request_id, "note": note})
    write_atomic(LEDGER, l)


# ---- calendar ----------------------------------------------------------------

def calendar() -> dict:
    return read(CALENDAR, {"slots": [], "generated_at": None})


def save_calendar(c: dict) -> None:
    write_atomic(CALENDAR, c)


def slot_dir(slot_id: str) -> Path:
    return config.DATA / "assets" / slot_id
