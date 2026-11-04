"""Quality control: duplicates, technical checks, and a vision judge for identity, compliance and
quality. The judge fails closed: when it cannot run, the image stays unapproved."""
import json
from pathlib import Path

import imagehash
from PIL import Image

from . import config, nim, state

JUDGE_SYSTEM = """You are a strict photo editor and compliance reviewer for an Instagram account of a
fictional, AI-generated adult woman. The first image(s) are the approved reference set of the
character. The LAST image is the candidate. Judge the candidate only.
Styling note: flattering, revealing, flirtatious barely-safe-for-work cuts (deep necklines, midriff
out, short hems, body-con silhouettes) are the intended look — they are NOT violations. What is a
violation: exposed nipples, genitals or buttocks, see-through over them, or an explicitly sexual act/pose.
Score honestly; false positives waste GPU time and false negatives get the account banned."""

JUDGE_USER = """Return JSON with exactly these keys:
- "same_person": 0-100, how certain the candidate shows the SAME person as the references (face shape,
  eyes, brows, nose, lips, the beauty marks on the right cheek, short bob with soft micro bangs mixing
  mint-green and soft lavender strands, freckles across nose and cheeks, silver septum ring, black nails).
- "hair_matches": true/false, the hair is the short pastel-goth bob with soft micro bangs in TWO tones —
  mint-green AND soft lavender strands both clearly present; false if the lavender strands are missing
  (solid single-color mint), or the color/cut/style differs from the references.
- "full_body_visible": true/false, the ENTIRE figure is visible head-to-toe — head AND feet fully inside
  the frame with margin; false when the shot is cropped at the chest, waist, hips or knees.
- "looks_adult": true/false (clearly an adult, no youthful or school styling).
- "nudity": true/false (any exposed nipple, genitals or buttocks, or see-through over them).
- "explicit_pose": true/false (sexual act or explicitly sexual pose; sultry/implied-sexy poses are fine).
- "hands_ok": true/false (no extra/missing/deformed fingers or limbs).
- "artifacts": true/false (text, watermark, logo, warped face, doubled features).
- "quality": 0-100 overall photographic quality for a premium fashion feed.
- "outfit_matches": true/false, judge COLORS and SILHOUETTE against this description: {outfit}
  (revealing/low-cut styling of the same outfit is intentional and still matches).
- "notes": one short sentence."""


class QcResult(dict):
    @property
    def approved(self) -> bool:
        return bool(self.get("approved"))


def phash(path: Path) -> str:
    with Image.open(path) as im:
        return str(imagehash.phash(im))


def is_duplicate(path: Path) -> tuple[bool, str | None]:
    """Compares against every hash the account has ever approved or posted."""
    h = imagehash.hex_to_hash(phash(path))
    for other_id, other_hex in state.read(state.HASHES, {}).items():
        if h - imagehash.hex_to_hash(other_hex) <= config.DUPLICATE_HAMMING_MAX:
            return True, other_id
    return False, None


def remember_hash(asset_id: str, path: Path) -> None:
    hashes = state.read(state.HASHES, {})
    hashes[asset_id] = phash(path)
    state.write_atomic(state.HASHES, hashes)


def technical(path: Path, aspect_ratio: str) -> list[str]:
    problems = []
    with Image.open(path) as im:
        w, h = im.size
    if min(w, h) < 1024:
        problems.append(f"too small: {w}x{h}")
    want_w, want_h = (int(x) for x in aspect_ratio.split(":"))
    if abs((w / h) - (want_w / want_h)) > 0.03:
        problems.append(f"aspect {w}x{h} is not {aspect_ratio}")
    return problems


def judge(candidate: Path, references: list[Path], outfit: str) -> dict:
    """The vision verdict. Raises nim.NimUnavailable when the judge cannot run."""
    return nim.chat_json(
        JUDGE_SYSTEM,
        JUDGE_USER.format(outfit=outfit),
        images=[*references[:3], candidate],
        temperature=0.1,
        max_tokens=config.NIM_JUDGE_TOKENS,
    )


def review(candidate: Path, references: list[Path], aspect_ratio: str, outfit: str, asset_id: str,
           require_full_body: bool = False) -> QcResult:
    """Runs every check. `approved` is true only when all pass; reasons list what failed."""
    reasons: list[str] = []
    reasons += technical(candidate, aspect_ratio)
    dup, other = is_duplicate(candidate)
    if dup:
        reasons.append(f"near-duplicate of {other}")

    verdict: dict = {}
    if not references:
        reasons.append("no approved reference set to compare against")
    else:
        try:
            verdict = judge(candidate, references, outfit)
        except (nim.NimUnavailable, json.JSONDecodeError) as e:
            reasons.append(f"judge unavailable: {e}")
        else:
            if int(verdict.get("same_person", 0)) < config.IDENTITY_MIN:
                reasons.append(f"identity {verdict.get('same_person')} < {config.IDENTITY_MIN}")
            if not verdict.get("hair_matches", True):
                reasons.append("hair does not match the two-tone mint/lavender bob")
            if require_full_body and not verdict.get("full_body_visible", True):
                reasons.append("not full body: head-to-toe framing not visible")
            if not verdict.get("looks_adult", False):
                reasons.append("does not read as an adult")
            if verdict.get("nudity") or verdict.get("explicit_pose"):
                reasons.append("nudity or explicit pose")
            if not verdict.get("hands_ok", False):
                reasons.append("hand or limb artifacts")
            if verdict.get("artifacts"):
                reasons.append("text, watermark or warping")
            if int(verdict.get("quality", 0)) < config.QUALITY_MIN:
                reasons.append(f"quality {verdict.get('quality')} < {config.QUALITY_MIN}")
            if not verdict.get("outfit_matches", True):
                reasons.append("outfit does not match the brief")

    result = QcResult(approved=not reasons, reasons=reasons, verdict=verdict, asset_id=asset_id)
    if result.approved:
        remember_hash(asset_id, candidate)
    return result


def thumbnail(path: Path, dest: Path, size: int = 320) -> Path:
    dest.parent.mkdir(parents=True, exist_ok=True)
    with Image.open(path) as im:
        im.thumbnail((size, size))
        im.convert("RGB").save(dest, "JPEG", quality=80)
    return dest
