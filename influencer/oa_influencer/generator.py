"""Image Generation Specialist: the reference set, then every calendar slot's images, each one
passed through QC and captioned in character. Generation is local (ComfyUI on the 4070)."""
import random
import re
import zlib
from pathlib import Path

from . import character, comfy, config, nim, planner, qc, state

REFERENCE_DIR = config.DATA / "reference"
# The references the FaceID adapter sees for every later image: front (face lock), FULL-BODY
# (composition exemplar — all-portrait refs biased every shot to bust-up crops, QC 2026-11-03),
# glam makeup. The adapter averages embeds, so identity holds while framing gets a head-to-toe cue.
IDENTITY_REFS = ("ref-front.jpg", "ref-full-body.jpg", "ref-glam.jpg")

# Framing clause for full-body shoots: forces head-to-toe visibility so SDXL cannot drift to a
# bust-up portrait (QC verdicts 2026-11-03).
FULL_BODY_FRAMING = ("full body shot, the ENTIRE figure visible head-to-toe: head, torso, legs and feet all "
                     "fully inside the frame with headroom above the hair and floor visible below the feet, "
                     "wide editorial framing, camera pulled back")

CAPTION_SYSTEM = """You write Instagram captions for Nova Vexley, a fictional AI-generated 24-year-old
dark-glam cosplayer. Voice: {tone}. Never claim she is real. Never mention prices or ask to buy.
No hashtags (they are added separately). 1-3 short lines. End with: {signoff}"""


def reference_paths() -> list[Path]:
    if not state.granted("reference_approved"):
        return []
    return [p for p in (REFERENCE_DIR / n for n in IDENTITY_REFS) if p.is_file()]


def _seed(name: str) -> int:
    """A stable seed per name, so a re-run of the reference set reproduces the same face."""
    return zlib.crc32(f"{character.spec()['slug']}:{name}".encode()) & 0x7FFF_FFFF


def generate_reference(session: comfy.Session) -> list[Path]:
    """Six canonical shots. The profile is generated first from the text alone; the other five
    use it as the FaceID reference so all six share one face. Eli approves the set once."""
    spec = character.spec()
    made = []
    profile = REFERENCE_DIR / "profile.jpg"
    for shot in character.reference_prompts(spec):
        dest = REFERENCE_DIR / f"{shot['name']}.jpg"
        if dest.exists():
            made.append(dest)
            continue
        aspect = {"profile": "1:1", "ref-front": "3:4", "ref-three-quarter": "3:4",
                  "ref-profile-left": "3:4", "ref-full-body": "3:4", "ref-glam": "1:1"}.get(shot["name"], "3:4")
        # Always FaceID-lock to profile/cand face so composition can vary via lower FaceID end_at
        face_lock = profile if profile.exists() else None
        refs = [face_lock] if face_lock else []
        out = session.images(shot["prompt"], shot["negative"], aspect, 1, REFERENCE_DIR, shot["name"],
                             references=refs, seed=_seed(shot["name"]))
        out[0].replace(dest)
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
    platform = slot.get("platform", "instagram")
    ip_safe = bool(slot.get("ip_safe"))
    outfit, setting, mood = theme["outfit"], theme["setting"], theme["mood"]
    extra_rule = ""
    if ip_safe:
        outfit = character.strip_character_names(outfit, spec)
        setting = character.strip_character_names(setting, spec)
        mood = character.strip_character_names(mood, spec)
        extra_rule = (" This is an IP-safe homage: never name the character, franchise, movie, game or studio "
                      "the look is inspired by; describe only the styling and mood.")
    if platform == "fanvue":
        system = ("You write Fanvue post captions for Nova Vexley, a fictional AI-generated 24-year-old dark-glam "
                  "cosplayer, for her subscribers. Voice: {tone}. Never claim she is real. Never mention prices "
                  "(the post price is shown separately) and never point to other platforms or payment methods. "
                  "1-3 short lines. End with: {signoff}").format(tone=v["tone"], signoff=v["signoff"])
    else:
        system = CAPTION_SYSTEM.format(tone=v["tone"], signoff=v["signoff"])
    text = nim.chat(
        system,
        f"Pillar: {slot['pillar']}. Look: {outfit}. Setting: {setting}. Mood: {mood}. "
        f"Story idea for the day: {slot['story']}.{extra_rule} Write the caption for the attached photo.",
        images=[image],
        temperature=0.8,
        max_tokens=config.NIM_CAPTION_TOKENS,
    )
    text = text.strip().strip('"')
    if ip_safe:
        text = character.strip_character_names(text, spec)
    if v["signoff"] not in text:
        text = f"{text}\n{v['signoff']}"
    return f"{text}\n\n{v['ai_disclosure']}\n\n{' '.join(slot['hashtags'])}"


def generate_slot(session: comfy.Session, slot: dict, refs: list[Path]) -> dict:
    """Full-body multi-image shoot in one outfit, max carousel size. Frames generate incrementally
    across runs (GEN_FRAMES_PER_RUN per run) so one post never blows the runtime job timeout;
    a slot goes ready only when every frame is QC-approved and captioned."""
    spec = character.spec()
    theme = character.theme(spec, slot["pillar"], slot["theme"])
    rng = random.Random(f"{slot['id']}:{slot['attempts']}")
    # Every post is a full-body photoshoot in the same outfit at IG's max images; distinct pose
    # per frame so the set reads as a photoshoot instead of one shot re-sampled (which QC would
    # flag as near-duplicates anyway).
    target = config.FEED_MAX_IMAGES if slot["kind"] == "carousel" else slot["images"]
    if slot["kind"] == "single" and slot["status"] == "planned":
        slot["kind"], target = "carousel", config.FEED_MAX_IMAGES
    slot["images"] = target
    already = [a for a in slot.get("assets", []) if a.get("approved")]
    remaining_frames = target - len(already)
    if remaining_frames <= 0:
        state.log(f"generate: {slot['id']} already has all {target} frames; going straight to caption")
        approved = already
        rejected = []
    else:
        # Full-body shoots only: every frame of a carousel is a head-to-toe shot (Eli format),
        # so portrait-crop poses ("three-quarter view...") are excluded from the pool entirely.
        base_pose = slot["pose"]
        pose_pool = [p for p in spec["poses"] if p.startswith("full body") and p != base_pose]
        if not pose_pool:
            pose_pool = [p for p in spec["poses"] if p != base_pose]
        pose = pose_pool[0] if slot["attempts"] == 0 else rng.choice(pose_pool)
        if not pose.startswith("full body"):
            pose = pose_pool[(slot["attempts"] - 1) % len(pose_pool)]  # never a portrait-crop pose
        slot["attempts"] += 1

        out_dir = state.slot_dir(slot["id"])
        stem = f"{slot['id']}-{slot['attempts']}"
        budget = min(remaining_frames, config.GEN_FRAMES_PER_RUN)
        approved, rejected = [], already
        for i in range(budget):
            if i == 0:
                frame_pose = pose
            else:
                frame_pose = pose_pool[(i - 1) % len(pose_pool)]
            prompt = character.image_prompt(spec, slot["pillar"], slot["theme"], frame_pose, glam=slot["glam"],
                                            framing=FULL_BODY_FRAMING if frame_pose.startswith("full body") else None,
                                            ip_safe=bool(slot.get("ip_safe")))
            paths = session.images(prompt["prompt"], prompt["negative"], slot["aspect_ratio"], 1, out_dir,
                                   f"{stem}-{i + 1}", references=refs)
            path = paths[0]
            asset_id = path.stem
            # The judge must check the CLOTHING: themes like pick-next-cosplay embed prop words in
            # their outfit brief, and SDXL routinely paraphrases props — props stay in the prompt
            # goal; only the outfit rejects. Alluring barely-SFW styling is intentional (2026-11-03).
            outfit_brief = re.sub(r",?\s*(holding|with prop|carrying)\b.*$", "", theme["outfit"], flags=re.I).strip(" ,")
            verdict = qc.review(path, refs, slot["aspect_ratio"], outfit_brief, asset_id,
                                require_full_body=frame_pose.startswith("full body"))
            qc.thumbnail(path, out_dir / "thumbs" / f"{asset_id}.jpg")
            entry = {"id": asset_id, "path": str(path), "approved": verdict.approved, "reasons": verdict["reasons"],
                     "verdict": verdict["verdict"], "prompt": prompt["prompt"], "pose": frame_pose}
            (approved if verdict.approved else rejected).append(entry)
            state.log(f"qc: {asset_id} {'approved' if verdict.approved else 'rejected: ' + '; '.join(verdict['reasons'])}")
        slot["assets"] = slot.get("assets", []) + [
            a for a in approved if a["id"] not in {x["id"] for x in already}
        ] + [a for a in rejected if a["id"] not in {x["id"] for x in already}]

    approved = [a for a in slot["assets"] if a.get("approved")]
    if len(approved) >= slot["images"]:
        try:
            slot["caption"] = slot["caption"] or caption_for(slot, Path(approved[0]["path"]))
        except nim.NimUnavailable as e:
            slot["status"] = "needs_caption"
            slot["last_error"] = str(e)
            state.log(f"caption: {slot['id']} deferred: {e}")
            return slot
        slot["status"] = "ready"
        slot["ready_at"] = state.iso()
        state.log(f"generate: {slot['id']} ready with {len(approved)} frames")
    elif approved:
        # shoots fill across runs: keep what passed, report the shortfall as in-progress
        slot["last_error"] = f"{len(approved)}/{slot['images']} frames approved so far (continues next run)"
    else:
        slot["status"] = "rejected"
        slot["last_error"] = "; ".join(r for a in rejected for r in a.get("reasons", []))[:300]
    return slot


def run(limit: int = 2) -> dict:
    """Generates assets for every due slot, soonest first. One ComfyUI session for the whole run."""
    cal = state.calendar()
    due = planner.due_for_generation(cal)[:limit]
    refs = reference_paths()
    summary = {"generated": 0, "ready": 0, "rejected": 0, "errors": []}
    if not due:
        return summary
    if not refs:
        summary["errors"].append("reference set not approved; run `pipeline.py reference` and approve it")
        return summary
    with comfy.Session() as session:
        for slot in due:
            try:
                generate_slot(session, slot, refs)
                summary["generated"] += 1
                summary["ready" if slot["status"] == "ready" else "rejected"] += 1
            except (comfy.ComfyError, ValueError) as e:
                slot["last_error"] = str(e)
                summary["errors"].append(f"{slot['id']}: {e}")
                state.log(f"generate: {slot['id']} failed: {e}")
                if isinstance(e, comfy.NotInstalled):
                    break
            finally:
                state.save_calendar(cal)
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
