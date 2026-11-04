"""The character spec and the prompts built from it. The identity block is identical in every
prompt; only outfit, setting, pose and makeup variant change."""
import json
import random
import re

from . import config


def spec() -> dict:
    return json.loads(config.CHARACTER_JSON.read_text(encoding="utf8"))


COSPLAY_LIST = config.ROOT / "character" / "cosplay_list.json"
PARODY_MAP = config.ROOT / "character" / "parody_map.json"
# Pillars that fan-cosplay the shared character pool from the list (never sold as prints: IP).
FAN_COSPLAY_PILLARS = ("famous_cosplays", "cosplay")


def fan_cosplay_pool() -> dict[str, dict]:
    """Eli's curated character list: name -> {category}. New names land under an existing
    category and join the round-robin pool automatically."""
    try:
        data = json.loads(COSPLAY_LIST.read_text(encoding="utf8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return {name: {"category": cat} for cat, names in data.get("categories", {}).items() for name in names}


def fan_theme_id(name: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")
    return f"fc:{slug}"


def parody_map() -> dict[str, dict]:
    """IP-safe parody briefs: name -> {look, prop, setting, mood}. Original looks that only
    SUGGEST the character — never the name, franchise or signature design."""
    try:
        data = json.loads(PARODY_MAP.read_text(encoding="utf8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return data.get("parodies", {})


def dynamic_theme(name: str) -> dict:
    """IP-safe parody theme for any list character: an original look that SUGGESTS the character
    via palette, silhouette family and a playful nod prop. The character's name, franchise and
    signature design never reach the prompt, caption or judge brief. Deterministic per name."""
    m = parody_map().get(name)
    if not m:
        pool = fan_cosplay_pool()
        cat = pool.get(name, {}).get("category", "classic").replace("_", " ")
        m = {"look": f"original dark-glam homage to a beloved {cat} heroine archetype, "
                     f"character-inspired color palette with gentle-goth black lace twist",
             "prop": "playful nod prop that suggests the archetype (original design)",
             "setting": f"editorial set styled around a {cat} heroine's world",
             "mood": "playful parody energy at her most confident"}
    return {
        "id": fan_theme_id(name),
        "name": name,
        "outfit": f"{m['look']} — parody homage that only suggests the archetype, never a replica of any existing design",
        "prop": m.get("prop"),
        "setting": m["setting"],
        "mood": m["mood"],
    }


def pillar(spec_: dict, pillar_id: str) -> dict:
    for p in spec_["pillars"]:
        if p["id"] == pillar_id:
            return p
    raise KeyError(pillar_id)


def theme_ids(spec_: dict, pillar_id: str) -> list[str]:
    """Themes a pillar can draw: Eli's curated fan-cosplay pool first, then the built-ins."""
    ids = [t["id"] for t in pillar(spec_, pillar_id)["themes"]]
    pool = [fan_theme_id(name) for name in fan_cosplay_pool()]
    return pool + ids if pillar_id in FAN_COSPLAY_PILLARS else ids


def theme(spec_: dict, pillar_id: str, theme_id: str) -> dict:
    for t in pillar(spec_, pillar_id)["themes"]:
        if t["id"] == theme_id:
            return t
    if pillar_id in FAN_COSPLAY_PILLARS and theme_id.startswith("fc:"):
        for name in fan_cosplay_pool():
            if fan_theme_id(name) == theme_id:
                return dynamic_theme(name)
    raise KeyError(f"{pillar_id}/{theme_id}")


def check_prompt(spec_: dict, prompt: str) -> list[str]:
    """Words the compliance rules forbid in any prompt. Empty list means clean."""
    lowered = prompt.lower()
    return [w for w in spec_["compliance"]["forbidden_in_prompt"] if w in lowered]


# ---- IP-safe parody mode (Fanvue surface) ------------------------------------
#
# Fan-cosplay content MAY go to Fanvue, but only as IP-safe parody: the look
# (silhouette, palette, styling cues) suggests the character; the character's
# name and franchise are never rendered or printed (Eli, 2026-11-03).
# Instagram keeps the full named-recreation treatment for feed engagement;
# Fanvue - a paid surface - gets the name-stripped homage.

_STOP_WORDS = {
    "the", "of", "and", "vs", "for", "forces", "evil", "family", "animated",
    "game", "games", "inspired", "style", "tv", "tva",
}
# Tokens that appear inside some character/franchise names but are too common
# in ordinary captions to ban ("Star Butterfly", "Raven (Titans)", "Sailor
# Moon", "Kim Possible", floral/plant aesthetics). The distinctive multi-word
# phrases stay banned regardless.
_GENERIC_SINGLE = {"star", "raven", "wednesday", "dc", "titans", "moon", "sailor", "beast", "fell"}
_COMMON_LAST = {"butterfly", "mouse", "duck", "possible", "pines", "storm", "girls", "bravo",
                "jack", "blossom", "bubbles", "griffin", "simpson"}
# Franchise/brand words that must never appear on the Fanvue paid surface either.
_FRANCHISE_EXTRA = {"disney", "nickelodeon", "nickelodeon's", "cartoon", "network", "warner",
                    "nintendo", "sega", "capcom", "square", "enix", "pixar", "hasbro",
                    "matel", "barbie", "hello", "kitty"}
# Highly distinctive first names from the pool that would otherwise only be caught as
# part of a full phrase ("Minnie Mouse", "Velma Dinkley").
_DISTINCTIVE_EXTRA = {"minnie", "velma", "wilma", "pebbles", "judy", "jetson", "jetsons",
                      "flintstone", "dinkley", "boop", "squarepants", "woodpecker"}


def fan_cosplay_name_patterns(spec_: dict | None = None) -> list[str]:
    """Case-insensitive phrases/tokens that must not appear in Fanvue prompts or captions:
    the distinctive names of every fan-cosplay character and franchise in the pool (and any
    name Eli later adds via `addcosplay`)."""
    spec_ = spec_ or spec()
    patterns: set[str] = set()

    def add_name(raw: str) -> None:
        for phrase in re.split(r"[/(),:]", (raw or "").lower()):
            words = [w for w in re.split(r"[^a-z0-9]+", phrase) if w]
            if not words:
                continue
            if len(words) >= 2:
                distinctive = [w for w in words if w not in _STOP_WORDS and w not in _GENERIC_SINGLE]
                if len(distinctive) >= 2:
                    patterns.add(" ".join(distinctive))  # "gravity falls", "mabel pines"
                last = words[-1]
                if len(last) >= 6 and last not in _COMMON_LAST and last not in _GENERIC_SINGLE \
                        and last not in _STOP_WORDS:
                    patterns.add(last)  # surnames/brand tokens: flintstone, dinkley, jetson
            else:
                w = words[0]
                if len(w) >= 4 and w not in _STOP_WORDS and w not in _GENERIC_SINGLE:
                    patterns.add(w)  # single-word characters: elsa, rapunzel, aerith, jinx

    for pillar_ in spec_.get("pillars", []):
        if not pillar_.get("fan_cosplay"):
            continue
        for t in pillar_.get("themes", []):
            add_name(t.get("character") or "")
            add_name(t.get("franchise") or "")
    for name in fan_cosplay_pool():  # Eli's curated list (98+ characters, addcosplay-extendable)
        add_name(name)
    patterns.update(_FRANCHISE_EXTRA)
    patterns.update(_DISTINCTIVE_EXTRA)
    patterns.update({"2b", "nier", "usagi", "scooby"})
    return sorted(p for p in patterns if p)


def strip_character_names(text: str, spec_: dict | None = None) -> str:
    """Removes the 'adult fan cosplay of X:' prefixes and every fan-cosplay name/franchise
    from the text, for IP-safe (Fanvue) prompts and captions."""
    out = re.sub(r"adult\s+fan\s+cosplay\s+of\s+[^:,.;]*[:;,.]?\s*", "", text, flags=re.IGNORECASE)
    for pattern in fan_cosplay_name_patterns(spec_):
        out = re.sub(rf"\b{re.escape(pattern)}\b", "", out, flags=re.IGNORECASE)
    return re.sub(r"\s{2,}", " ", out).strip(" ,-:")


def image_prompt(spec_: dict, pillar_id: str, theme_id: str, pose: str, glam: bool = False,
                 framing: str | None = None, ip_safe: bool = False) -> dict:
    """Positive and negative prompts for one shot. Raises if the result would violate the rules.
    ip_safe=True (Fanvue): outfit/setting/prop texts are name-stripped and the named-character
    clause is replaced by a look-only homage clause."""
    a = spec_["appearance"]
    t = theme(spec_, pillar_id, theme_id)
    if ip_safe:
        t = {**t, "outfit": strip_character_names(t["outfit"], spec_),
             "setting": strip_character_names(t.get("setting", ""), spec_),
             "mood": strip_character_names(t.get("mood", ""), spec_),
             "prop": strip_character_names(t.get("prop", ""), spec_) if t.get("prop") else None}
    makeup = a["makeup_glam"] if glam else a["makeup_default"]
    full_body = bool(framing and "full body" in framing.lower())
    lead = "photorealistic editorial photo, wide full-body shot" if full_body else "photorealistic editorial portrait"
    parts = [
        # outfit words first: when they trail the ~60-token identity/appearance block, SDXL
        # drifts the wardrobe (green turtleneck for black, tank top for turtleneck — QC verdicts
        # 2026-11-03); identity stays locked by the IP-Adapter either way.
        f"{lead}, the woman is wearing {t['outfit']} (outfit exactly as described, correct colors)",
        a.get("styling_allure", ""),
        a["identity_prompt"],
        makeup,
    ]
    if t.get("prop"):
        parts.append(f"wearing the outfit above, holding or with prop: {t['prop']}")
    if ip_safe:
        parts.append("suggestive homage styling that evokes the look's silhouette, palette and props only; "
                     "original parody interpretation, no character names, no trademarks, no logos")
    elif t.get("character"):
        parts.append(f"adult fan cosplay of {t['character']} (homage, not official)")
    parts += [
        f"{t['setting']}",
        f"mood: {t['mood']}",
        pose,
        a["lighting_signature"],
        a["camera"],
        "adult woman, fully covered where required, tasteful, no nudity",
    ]
    if full_body:
        # 85mm and shallow depth of field are portrait-crop cues; replace them inline.
        parts = [p.replace("shallow depth of field, 85mm lens look", "deep focus, wide framing")
                  .replace("85mm f/1.8", "35mm f/2.8") for p in parts]
    # Hair drift guard: SDXL under-weights the two-tone clause buried in the identity block, so the
    # mint/lavender mix degrades to solid mint (QC verdicts 2026-11-03). Repeated at the end for
    # recency; the framing clause goes up FRONT for the same reason (SDXL chunks long prompts and
    # the tail is nearly ignored — head-to-toe clause at the end changed nothing, QC 2026-11-03).
    if framing:
        parts.insert(1, framing)
    parts.append("hair exactly as described: short pastel-goth bob with soft micro bangs, two-tone "
                 "mint-green bob mixed with soft lavender strands, both colors clearly present")
    positive = ", ".join(p for p in parts if p)
    negative = a["negative"] + ", wrong outfit color, outfit not matching the description"
    if full_body:
        negative += ", bust-up portrait, head-and-shoulders crop, cropped at chest, waist or knees, close-up face only, cropped feet"
    bad = check_prompt(spec_, positive)
    if bad:
        raise ValueError(f"Prompt violates compliance rules: {bad}")
    return {"prompt": positive, "negative": negative, "pillar": pillar_id, "theme": theme_id, "pose": pose,
            "glam": glam, "ip_safe": ip_safe}


def reference_prompts(spec_: dict) -> list[dict]:
    """Six genuinely different refs: pose words first, gentle-goth outfits, portrait aspects.
    Face lock comes from IP-Adapter FaceID on the locked cand_4 source, not from a pastel turtleneck.
    """
    a = spec_["appearance"]
    base_id = a["identity_prompt"]
    makeup = a["makeup_default"]
    glam_mu = a["makeup_glam"]
    light = a.get("lighting_signature") or "soft flattering editorial light"
    cam = a["camera"]
    cover = "adult woman, fully covered, no nudity, tasteful"
    # Shared gentle-goth wardrobe language (no pastel turtleneck)
    goth = (
        "gentle goth outfit: black lace and velvet, soft corset detail, choker, "
        "dark romantic styling, platform boots when legs visible"
    )
    neg = a["negative"]
    for extra in (
        "pastel turtleneck", "plain turtleneck", "identical pose", "same crop",
        "duplicate composition", "cut off feet", "cropped at knees for full body",
    ):
        if extra.lower() not in neg.lower():
            neg = neg + ", " + extra

    def shot(name, pose_first, outfit_extra, aspect_hint="portrait"):
        parts = [
            pose_first,  # pose words FIRST
            base_id,
            glam_mu if name == "ref-glam" else makeup,
            goth,
            outfit_extra,
            light,
            cam,
            cover,
            "soft pretty gentle goth aesthetic",
        ]
        return {
            "name": name,
            "prompt": ", ".join(p for p in parts if p),
            "negative": neg,
            "aspect_hint": aspect_hint,
        }

    return [
        shot(
            "profile",
            "head-and-shoulders portrait, face toward camera, soft eye contact, gentle smile, tight head-and-shoulders crop",
            "black velvet top with lace sleeves, delicate silver choker, soft dark hair ribbons",
            "1:1",
        ),
        shot(
            "ref-front",
            "waist-up front portrait, facing camera straight on, hands relaxed near waist, open posture",
            "black lace blouse under a soft velvet corset belt, choker, subtle silver jewellery",
            "3:4",
        ),
        shot(
            "ref-three-quarter",
            "three-quarter view portrait, body angled 45 degrees, looking back toward camera over the near shoulder",
            "black velvet off-shoulder top with lace trim, layered choker, soft romantic sleeves",
            "3:4",
        ),
        shot(
            "ref-profile-left",
            "true side profile portrait, full left profile silhouette, nose and chin cleanly edged, looking left, not three-quarter",
            "black lace high collar with velvet panel, thin choker, green and lavender bob clearly in profile",
            "3:4",
        ),
        shot(
            "ref-full-body",
            "full body head-to-feet portrait standing, entire figure in frame from hair to platform boot soles, "
            "clearly short petite small-frame adult woman about 5 foot 1, small proportions, weight on one hip, "
            "empty space above head and below boots, studio full-length",
            "black velvet mini dress with lace sleeves and soft corset waist, choker, opaque tights, black platform boots",
            "3:4",
        ),
        shot(
            "ref-glam",
            "close glam portrait, slight chin tilt, soft sultry gentle expression, beauty lighting on face",
            "black satin and lace glam top, statement choker, soft smoked gentle-goth glam makeup",
            "1:1",
        ),
    ]



def pick_pose(spec_: dict, rng: random.Random) -> str:
    return rng.choice(spec_["poses"])


def hashtags(spec_: dict, pillar_id: str, rng: random.Random) -> list[str]:
    pools = spec_["hashtag_pools"]
    core = rng.sample(pools["core"], 5)
    own = rng.sample(pools.get(pillar_id, []), min(3, len(pools.get(pillar_id, []))))
    return core + own
