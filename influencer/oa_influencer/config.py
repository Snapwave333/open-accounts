"""Paths, keys and limits. Keys come from the environment or the Windows user environment."""
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHARACTER_JSON = ROOT / "character" / "character.json"
DATA = Path(os.environ.get("OPENACCOUNTS_INFLUENCER_DATA") or ROOT / "data")
STUDIO_ROOT = Path(os.environ.get("OPENACCOUNTS_STUDIO_ROOT") or r"C:\Users\chrom\Desktop\StickExplainers")

NIM_CHAT = "https://integrate.api.nvidia.com/v1/chat/completions"
NIM_VISION_MODEL = "nvidia/nemotron-3.5-super-vl-120b-a12b"
NIM_TEXT_MODEL = "nvidia/nemotron-3.5-super-vl-120b-a12b"
# nemotron is a reasoning model: reasoning_content is billed against max_tokens, so the
# judge and caption calls need headroom past the thinking phase or content comes back empty.
NIM_JUDGE_TOKENS = 4000
NIM_CAPTION_TOKENS = 1200
NIM_REPLY_TOKENS = 3000
# The policy researcher reads ~95KB of legal documents and must think AND emit a full
# JSON ruleset: give it a generous budget or the thinking phase eats the reply.
NIM_RESEARCH_TOKENS = 14000

# Publishing rhythm (also enforced by the publisher, not only the planner).
# Rate & ToS guardrails (Instagram, checked against the platform-intelligence rules the
# app ships): 1 feed post/day spaced >=20h, curated-follow growth at <=5/day with live-profile
# verification and no unfollow loops, in-character replies only on Nova's own posts at <=10/day,
# no automated outbound likes/DMs, AI label on every post. Anything that violates the automation
# rules ("bulk" actions, engagement pods, scraping) stays out by design.
FEED_POSTS_PER_DAY = 1
MIN_FEED_GAP_HOURS = 20
POST_HOUR_LOCAL = 19
GENERATION_LEAD_DAYS = 3
MAX_GENERATION_ATTEMPTS = 6
REPLIES_PER_DAY_MAX = 10
GROW_FOLLOWS_PER_DAY = 5
# Every feed post is a maximum-size carousel (IG's current multi-image cap).
FEED_MAX_IMAGES = 20
# GPU pacing: ComfyUI + QC renders a few frames per generate run; a slot fills across runs
# (6h cadence, 2h job timeout). Keeps every run well inside its timeout.
GEN_FRAMES_PER_RUN = 8

# ---- Fanvue surface (separate calendar/policy from Instagram; Eli, 2026-11-03) ----
# Two-platform funnel: Instagram keeps 20-image carousels + engagement; Fanvue carries the
# spicy/monetized side. Values mirror the supplemental strategy spec: 1-2 posts/day,
# 5-7 posting days/week, themed shoots of 5-10 images, 3-5 PPV releases/week.
FANVUE_POSTS_PER_DAY = 2
FANVUE_MIN_GAP_HOURS = 4
FANVUE_SHOOT_MIN_IMAGES = 5
FANVUE_SHOOT_MAX_IMAGES = 10
FANVUE_PPV_MIN_PER_WEEK = 3
FANVUE_PPV_MAX_PER_WEEK = 5
FANVUE_PPV_PRICE_CENTS = (499, 999)   # recommendation range per set size
FANVUE_SUBSCRIPTION_CENTS = 999        # $9.99/month starting tier
FANVUE_TIMEZONE = "America/Denver"     # test windows 11:00-13:00 and 18:00-21:00 local
FANVUE_TEST_WINDOWS = {"midday": (11, 0, 13, 0), "evening": (18, 0, 21, 0)}


# Curated fan-cosplay pillar (Eli's character list) keeps a guaranteed share of the calendar.
FAMOUS_WEIGHT_FLOOR = 20


def publish_pace() -> str:
    return f"{FEED_POSTS_PER_DAY}/day, {MIN_FEED_GAP_HOURS}h gap"

# Quality thresholds for the vision judge (0-100).
IDENTITY_MIN = 75
QUALITY_MIN = 70
DUPLICATE_HAMMING_MAX = 6

# Retention: posted media is deleted this many hours after Instagram confirms the post.
KEEP_POSTED_HOURS = 24
KEEP_REJECTED_HOURS = 48

# Instagram routing for Nova (@nova_a_aa_).
# "relay"  = Chrome browser relay (NovaChrome profile).
# "emulator" = headless Pixel7 emulator (emulator-5554), Instagram app signed in as
#   @nova_a_aa_ only — never @snapwave. Sign-in is done, so the emulator is active now.
INSTAGRAM_VIA = os.environ.get("OPENACCOUNTS_INSTAGRAM_VIA", "relay")
NOVA_HANDLE = "nova_a_aa_"
NOVA_USER_ID = 18697249737


def _user_env(name: str) -> str | None:
    """A variable from the process environment, else from the Windows user environment (registry),
    so keys set in System Properties work for processes started before the shell saw them."""
    value = os.environ.get(name)
    if value and value.strip():
        return value.strip()
    if sys.platform != "win32":
        return None
    try:
        import winreg

        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment") as key:
            value = winreg.QueryValueEx(key, name)[0]
            return value.strip() or None
    except OSError:
        return None


def nvidia_key() -> str | None:
    return _user_env("NVIDIA_API_KEY")
