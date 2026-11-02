"""The character spec and the prompts built from it. The identity block is identical in every
prompt; only outfit, setting, pose and makeup variant change."""
import json
import random

from . import config


def spec() -> dict:
    return json.loads(config.CHARACTER_JSON.read_text(encoding="utf8"))


def pillar(spec_: dict, pillar_id: str) -> dict:
    for p in spec_["pillars"]:
        if p["id"] == pillar_id:
            return p
    raise KeyError(pillar_id)


def theme(spec_: dict, pillar_id: str, theme_id: str) -> dict:
    for t in pillar(spec_, pillar_id)["themes"]:
        if t["id"] == theme_id:
            return t
    raise KeyError(f"{pillar_id}/{theme_id}")


def check_prompt(spec_: dict, prompt: str) -> list[str]:
    """Words the compliance rules forbid in any prompt. Empty list means clean."""
    lowered = prompt.lower()
    return [w for w in spec_["compliance"]["forbidden_in_prompt"] if w in lowered]


def image_prompt(spec_: dict, pillar_id: str, theme_id: str, pose: str, glam: bool = False) -> dict:
    """Positive and negative prompts for one shot. Raises if the result would violate the rules."""
    a = spec_["appearance"]
    t = theme(spec_, pillar_id, theme_id)
    makeup = a["makeup_glam"] if glam else a["makeup_default"]
    positive = ", ".join(
        [
            a["identity_prompt"],
            makeup,
            f"wearing {t['outfit']}",
            f"{t['setting']}",
            f"mood: {t['mood']}",
            pose,
            a["lighting_signature"],
            a["camera"],
            "adult woman, fully covered where required, tasteful, no nudity",
        ]
    )
    bad = check_prompt(spec_, positive)
    if bad:
        raise ValueError(f"Prompt violates compliance rules: {bad}")
    return {"prompt": positive, "negative": a["negative"], "pillar": pillar_id, "theme": theme_id, "pose": pose, "glam": glam}


def reference_prompts(spec_: dict) -> list[dict]:
    """The canonical reference set: neutral looks that show the face, hair and marks clearly."""
    a = spec_["appearance"]
    base = [a["identity_prompt"], a["makeup_default"], a["camera"], "adult woman, fully covered, no nudity"]
    shots = [
        ("profile", "close-up headshot, eyes to camera, neutral expression, plain black fitted turtleneck, "
                    "grey seamless studio background, soft key light with violet rim light, 1:1"),
        ("ref-front", "head and shoulders, facing camera, faint smirk, black turtleneck, grey studio background, soft even light"),
        ("ref-three-quarter", "three-quarter view, chin slightly down, black turtleneck, grey studio background, soft even light"),
        ("ref-profile-left", "left profile showing the platinum streak and the crescent moon tattoo behind the ear, black turtleneck, grey studio background"),
        ("ref-full-body", "full body standing, weight on one hip, black turtleneck and black trousers, boots, grey studio background, even light"),
        ("ref-glam", f"close-up, {a['makeup_glam']}, black satin top, hard key light with violet rim light"),
    ]
    return [{"name": name, "prompt": ", ".join(base + [detail]), "negative": a["negative"]} for name, detail in shots]


def pick_pose(spec_: dict, rng: random.Random) -> str:
    return rng.choice(spec_["poses"])


def hashtags(spec_: dict, pillar_id: str, rng: random.Random) -> list[str]:
    pools = spec_["hashtag_pools"]
    core = rng.sample(pools["core"], 5)
    own = rng.sample(pools.get(pillar_id, []), min(3, len(pools.get(pillar_id, []))))
    return core + own
