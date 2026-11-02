"""Image Generation Specialist: the reference set, then every calendar slot's images, each one
passed through QC and captioned in character."""
import random
from pathlib import Path

from . import character, config, higgsfield, nim, planner, qc, state

REFERENCE_DIR = config.DATA / "reference"

CAPTION_SYSTEM = """You write Instagram captions for Nova Vexley, a fictional AI-generated 24-year-old
dark-glam cosplayer. Voice: {tone}. Never claim she is real. Never mention prices or ask to buy.
No hashtags (they are added separately). 1-3 short lines. End with: {signoff}"""


def reference_paths() -> list[Path]:
    if not state.granted("reference_approved"):
        return []
    return sorted(p for p in REFERENCE_DIR.glob("ref-*.jpg"))


def generate_reference(session: higgsfield.Session) -> list[Path]:
    """Six canonical shots. They are the identity: Eli approves them once, then every later image
    is judged against them."""
    spec = character.spec()
    made = []
    for shot in character.reference_prompts(spec):
        dest = REFERENCE_DIR / f"{shot['name']}.jpg"
        if dest.exists():
            made.append(dest)
            continue
        aspect = "1:1" if shot["name"] in ("profile", "ref-front") else "3:4"
        result = session.images(shot["prompt"], aspect, n=1, note=f"reference {shot['name']}")
        url = result["images"][0]["url"]
        higgsfield.download(url, dest)
        made.append(dest)
        state.log(f"reference: {dest.name} saved")
    st = state.state()
    st["reference"] = {"dir": str(REFERENCE_DIR), "files": [p.name for p in made], "generated_at": state.iso()}
    state.save_state(st)
    return made


def caption_for(slot: dict, image: Path) -> str:
    spec = character.spec()
    v = spec["voice"]
    theme = character.theme(spec, slot["pillar"], slot["theme"])
    text = nim.chat(
        CAPTION_SYSTEM.format(tone=v["tone"], signoff=v["signoff"]),
        f"Pillar: {slot['pillar']}. Look: {theme['outfit']}. Setting: {theme['setting']}. Mood: {theme['mood']}. "
        f"Story idea for the day: {slot['story']}. Write the caption for the attached photo.",
        images=[image],
        temperature=0.8,
        max_tokens=200,
    )
    text = text.strip().strip('"')
    if v["signoff"] not in text:
        text = f"{text}\n{v['signoff']}"
    return f"{text}\n\n{v['ai_disclosure']}\n\n{' '.join(slot['hashtags'])}"


def generate_slot(session: higgsfield.Session, slot: dict, refs: list[Path]) -> dict:
    spec = character.spec()
    theme = character.theme(spec, slot["pillar"], slot["theme"])
    rng = random.Random(f"{slot['id']}:{slot['attempts']}")
    pose = slot["pose"] if slot["attempts"] == 0 else character.pick_pose(spec, rng)
    prompt = character.image_prompt(spec, slot["pillar"], slot["theme"], pose, glam=slot["glam"])
    slot["attempts"] += 1
    reference_id = state.state().get("character_reference_id")

    result = session.images(
        prompt["prompt"], slot["aspect_ratio"], n=slot["images"], note=slot["id"], reference_id=reference_id
    )
    out_dir = state.slot_dir(slot["id"])
    approved, rejected = [], []
    for i, img in enumerate(result.get("images", []), start=1):
        asset_id = f"{slot['id']}-{slot['attempts']}-{i}"
        path = higgsfield.download(img["url"], out_dir / f"{asset_id}.jpg")
        verdict = qc.review(path, refs, slot["aspect_ratio"], theme["outfit"], asset_id)
        qc.thumbnail(path, out_dir / "thumbs" / f"{asset_id}.jpg")
        entry = {"id": asset_id, "path": str(path), "approved": verdict.approved, "reasons": verdict["reasons"],
                 "verdict": verdict["verdict"], "prompt": prompt["prompt"], "pose": pose}
        (approved if verdict.approved else rejected).append(entry)
        state.log(f"qc: {asset_id} {'approved' if verdict.approved else 'rejected: ' + '; '.join(verdict['reasons'])}")

    slot["assets"] = slot.get("assets", []) + approved + rejected
    if len(approved) >= slot["images"] or (approved and slot["kind"] == "single"):
        try:
            slot["caption"] = caption_for(slot, Path(approved[0]["path"]))
        except nim.NimUnavailable as e:
            slot["status"] = "needs_caption"
            slot["last_error"] = str(e)
            state.log(f"caption: {slot['id']} deferred: {e}")
            return slot
        slot["status"] = "ready"
        slot["ready_at"] = state.iso()
    elif approved:
        # a carousel with too few approved frames: keep what passed, try again for the rest
        slot["status"] = "rejected"
        slot["last_error"] = f"{len(approved)}/{slot['images']} frames approved"
    else:
        slot["status"] = "rejected"
        slot["last_error"] = "; ".join(r for a in rejected for r in a["reasons"])[:300]
    return slot


def run(limit: int = 2) -> dict:
    """Generates assets for every due slot, newest first, within the budget."""
    cal = state.calendar()
    due = planner.due_for_generation(cal)[:limit]
    refs = reference_paths()
    summary = {"generated": 0, "ready": 0, "rejected": 0, "skipped": 0, "errors": []}
    if not due:
        return summary
    if not refs:
        summary["errors"].append("reference set not approved; run `pipeline.py reference` and approve it")
        return summary
    session = higgsfield.Session()
    for slot in due:
        try:
            generate_slot(session, slot, refs)
            summary["generated"] += 1
            summary["ready" if slot["status"] == "ready" else "rejected"] += 1
        except higgsfield.BudgetExceeded as e:
            summary["errors"].append(str(e))
            state.log(f"generate: stopping, {e}")
            break
        except (higgsfield.HiggsfieldError, ValueError) as e:
            slot["last_error"] = str(e)
            summary["errors"].append(f"{slot['id']}: {e}")
            state.log(f"generate: {slot['id']} failed: {e}")
        finally:
            state.save_calendar(cal)
    summary["spent_usd"] = round(session.spent, 4)
    return summary


def caption_pending() -> int:
    """Captions for slots whose images passed but NIM was down at the time."""
    cal = state.calendar()
    n = 0
    for slot in cal["slots"]:
        if slot["status"] != "needs_caption":
            continue
        first = next((a for a in slot["assets"] if a["approved"]), None)
        if not first:
            continue
        try:
            slot["caption"] = caption_for(slot, Path(first["path"]))
            slot["status"] = "ready"
            slot["ready_at"] = state.iso()
            n += 1
        except nim.NimUnavailable as e:
            slot["last_error"] = str(e)
    state.save_calendar(cal)
    return n
