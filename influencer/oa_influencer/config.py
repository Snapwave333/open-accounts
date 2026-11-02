"""Paths, keys and limits. Keys come from the environment or the Windows user environment."""
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHARACTER_JSON = ROOT / "character" / "character.json"
DATA = Path(os.environ.get("OPENACCOUNTS_INFLUENCER_DATA") or ROOT / "data")
STUDIO_ROOT = Path(os.environ.get("OPENACCOUNTS_STUDIO_ROOT") or r"C:\Users\chrom\Desktop\StickExplainers")

HIGGSFIELD_API = "https://api.higgsfield.ai"
NIM_CHAT = "https://integrate.api.nvidia.com/v1/chat/completions"
NIM_VISION_MODEL = "nvidia/nemotron-3.5-super-vl-120b-a12b"
NIM_TEXT_MODEL = "nvidia/nemotron-3.5-super-vl-120b-a12b"

# Model paths on the Higgsfield platform (the same ones Open Higgsfield submits to).
SOUL_MODEL = os.environ.get("OPENACCOUNTS_SOUL_MODEL", "higgsfield-ai/soul/v2/standard")
I2V_MODEL = os.environ.get("OPENACCOUNTS_I2V_MODEL", "kling-video/v3.0-turbo/image-to-video")

# Spending guard: the pipeline estimates every request first and refuses past these.
DAILY_USD_CAP = float(os.environ.get("OPENACCOUNTS_INFLUENCER_DAILY_USD", "3.00"))
RUN_USD_CAP = float(os.environ.get("OPENACCOUNTS_INFLUENCER_RUN_USD", "1.50"))

# Publishing rhythm (also enforced by the publisher, not only the planner).
FEED_POSTS_PER_DAY = 1
MIN_FEED_GAP_HOURS = 20
POST_HOUR_LOCAL = 19
REEL_WEEKDAY = 3  # Thursday
REEL_START_AFTER_POSTS = 7
GENERATION_LEAD_DAYS = 3
MAX_GENERATION_ATTEMPTS = 3

# Quality thresholds for the vision judge (0-100).
IDENTITY_MIN = 75
QUALITY_MIN = 70
DUPLICATE_HAMMING_MAX = 6

# Retention: posted media is deleted this many hours after Instagram confirms the post.
KEEP_POSTED_HOURS = 24
KEEP_REJECTED_HOURS = 48


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


def higgsfield_key() -> str | None:
    """`id:secret` for the Higgsfield platform, or None when Eli has not added one yet."""
    key = _user_env("HIGGSFIELD_API_KEY")
    if key and ":" in key:
        return key
    return None


def nvidia_key() -> str | None:
    return _user_env("NVIDIA_API_KEY")
