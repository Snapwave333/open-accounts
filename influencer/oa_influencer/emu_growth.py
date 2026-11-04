"""Instagram growth (following creators) for Nova (@nova_a_aa_) through the headless
Pixel7 emulator. Mirrors growth.py's follow_one but drives the Instagram app directly
via adb/uiautomator. Never touches @snapwave.

   from . import emu_growth
   status = emu_growth.follow_one(creator)  # "clicked" | "already" | "missing" | "no-follow-button"
"""
import re
import time

from . import config, state
from . import emu as _e


def _open_creator(creator: str):
    _e.open_profile()
    # use search to jump to the creator's profile
    search = _e.find(["Search"], exact=True)
    if search:
        _e.sh(f"input tap {search['x']} {search['y']}"); time.sleep(2)
    box = _e.find(["Search", "Search Instagram"])
    if box:
        _e.sh(f"input tap {box['x']} {box['y']}"); time.sleep(1.5)
        _e.sh("input keycombination 113 29; input keyevent KEYCODE_DEL")
        _e.type_text(creator.encode("ascii", "ignore").decode(), slow=True)
        time.sleep(2)
    # tap the top result that matches the creator
    for _ in range(4):
        ns = _e.nodes()
        _e.guard(ns)
        n = _e.find([creator], ns, exact=False)
        if n:
            _e.sh(f"input tap {n['x']} {n['y']}"); time.sleep(3)
            _e.onboarding()
            return True
        _e.sh("input swipe 540 1200 540 700 400"); time.sleep(2)
    return False


def follow_one(creator: str) -> str:
    """Open `creator`'s profile in the emulator and press Follow. Returns a status word."""
    if not _open_creator(creator):
        return "missing"
    ns = _e.nodes()
    # not-found check
    blob = " ".join((n["text"] or "") + " " + (n["desc"] or "") for n in ns)
    if re.search(r"page isn't available|Sorry, this page|No results|User not found", blob, re.I):
        return "missing"
    # look for a Follow button
    for _ in range(4):
        ns = _e.nodes()
        _e.guard(ns)
        for n in ns:
            t = (n["text"] or "").strip().lower()
            if t == "follow" and n["w"] > 0:
                _e.sh(f"input tap {n['x']} {n['y']}"); time.sleep(2)
                return "clicked"
            if t in ("following", "follow back", "requested"):
                return "already"
        # header may already say Following
        hdr = [x for x in ns if x["rid"].endswith("action_bar_title") and x["text"]]
        if hdr and config.NOVA_HANDLE in (hdr[0]["text"] or ""):
            return "already"
        _e.sh("input swipe 540 1400 540 900 300"); time.sleep(2)
    return "no-follow-button"
