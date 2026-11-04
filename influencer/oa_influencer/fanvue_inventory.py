"""Content inventory for the Fanvue surface: every generated, QC-approved asset is tracked
here with hashes so the strategist never publishes the same image twice, reuses near-
duplicates inside one release, or lets a themed shoot go stale. PPV releases are curated
from this pool (5-10 carefully selected images), never flooded.

Statuses: available -> reserved (in an assembled PPV set) -> posted (feed/ppv) | vault.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

from . import config, qc, state

INVENTORY = config.DATA / "fanvue_inventory.json"

# A set selected for one release must not contain two frames closer than this
# (same rule the QC duplicate check uses against the approved history).
SET_HAMMING_MAX = config.DUPLICATE_HAMMING_MAX


def load() -> dict:
    return state.read(INVENTORY, {"assets": [], "updated_at": None})


def save(inv: dict) -> None:
    inv["updated_at"] = state.iso()
    state.write_atomic(INVENTORY, inv)


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def register_slot_assets(slot: dict) -> int:
    """Registers every approved asset of a generated slot. Exact duplicates (same sha256)
    are skipped; the entry records theme/pillar for later curation. Returns count added."""
    inv = load()
    known = {a["sha256"] for a in inv["assets"] if a.get("sha256")}
    added = 0
    for asset in slot.get("assets", []):
        if not asset.get("approved"):
            continue
        path = Path(asset["path"])
        if not path.is_file():
            continue
        digest = _sha256(path)
        if digest in known:
            continue
        try:
            perceptual = qc.phash(path)
        except Exception:
            perceptual = ""
        inv["assets"].append({
            "id": asset["id"],
            "path": str(path),
            "sha256": digest,
            "phash": perceptual,
            "slot": slot["id"],
            "pillar": slot.get("pillar"),
            "theme": slot.get("theme"),
            "ip_safe": bool(slot.get("ip_safe")),
            "status": "available",
            "used_in": None,
            "created_at": state.iso(),
        })
        known.add(digest)
        added += 1
    save(inv)
    if added:
        state.log(f"inventory: +{added} from {slot['id']} (total {len(inv['assets'])})")
    return added


def _hamming(a: str, b: str) -> int | None:
    if not a or not b:
        return None
    try:
        import imagehash

        return imagehash.hex_to_hash(a) - imagehash.hex_to_hash(b)
    except Exception:
        return None


def available(limit: int, theme: str | None = None, exclude_ids: set[str] | None = None) -> list[dict]:
    """Oldest-first available assets, skipping near-duplicates of anything already picked
    (a release must read as a photoshoot, not one shot re-sampled)."""
    inv = load()
    exclude_ids = exclude_ids or set()
    picked: list[dict] = []
    for asset in inv["assets"]:
        if len(picked) >= limit:
            break
        if asset.get("status") != "available" or asset["id"] in exclude_ids:
            continue
        if theme and asset.get("theme") != theme:
            continue
        if not Path(asset["path"]).is_file():
            asset["status"] = "missing"
            continue
        dup = None
        for other in picked:
            d = _hamming(asset.get("phash", ""), other.get("phash", ""))
            if d is not None and d <= SET_HAMMING_MAX:
                dup = other["id"]
                break
        if dup:
            continue
        picked.append(asset)
    save(inv)  # persist any missing-status transitions
    return picked


def reserve(ids: list[str], slot_id: str) -> None:
    inv = load()
    for asset in inv["assets"]:
        if asset["id"] in ids and asset.get("status") == "available":
            asset["status"] = "reserved"
            asset["used_in"] = slot_id
    save(inv)


def mark_posted(ids: list[str], slot_id: str, status: str = "posted") -> None:
    inv = load()
    for asset in inv["assets"]:
        if asset["id"] in ids:
            asset["status"] = status
            asset["used_in"] = slot_id
    save(inv)


def release(ids: list[str]) -> None:
    """Returns reserved assets to the pool (a scheduled PPV slot that was re-planned)."""
    inv = load()
    for asset in inv["assets"]:
        if asset["id"] in ids and asset.get("status") == "reserved":
            asset["status"] = "available"
            asset["used_in"] = None
    save(inv)


def stats() -> dict:
    inv = load()
    by_status: dict[str, int] = {}
    by_theme: dict[str, int] = {}
    for a in inv["assets"]:
        by_status[a.get("status", "?")] = by_status.get(a.get("status", "?"), 0) + 1
        if a.get("status") == "available":
            by_theme[a.get("theme") or "?"] = by_theme.get(a.get("theme") or "?", 0) + 1
    return {"total": len(inv["assets"]), "by_status": by_status, "available_by_theme": by_theme}
