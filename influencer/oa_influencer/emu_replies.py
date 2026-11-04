"""Instagram engagement for Nova (@nova_a_aa_) through the headless Pixel7 emulator.

Mirrors the relay-based discovery/send in replies.py but drives the Instagram app
directly via adb/uiautomator. Never touches @snapwave.

   from . import emu_replies
   targets = emu_replies.fetch_targets()   # [{url, path, comments:[{author,text}]}]
   outcome = emu_replies.send_one(comment, post_path, reply_text, dry=dry)
"""
import json
import re
import time
from pathlib import Path

from . import config, state
from . import emu as _e  # low-level adb helpers + ensure_account()


def _find_post_links(ns):
    """Extract /p/ or /reel/ hrefs from a uiautomator dump (Instagram app text links)."""
    out = []
    seen = set()
    for n in ns:
        for v in (n["text"], n["desc"]):
            if not v:
                continue
            for m in re.finditer(r"/(?:p|reel)/[\w-]+/?", v):
                href = m.group(0)
                if href not in seen:
                    seen.add(href)
                    out.append(href)
    return out[:12]


def _find_comments(ns, me):
    """Pull {author, text} comment blocks from a post's uiautomator dump."""
    comments = []
    seen = set()
    # Comments render as user+text pairs; match text lines that look like a comment
    # following a handle-like line.
    texts = [(n["text"] or "").strip() for n in ns if (n["text"] or "").strip()]
    for i, t in enumerate(texts):
        if t == me or t.startswith("http") or t.startswith("@"):
            continue
        if len(t) < 2 or len(t) > 300:
            continue
        # try to pair with a preceding handle
        author = ""
        for j in range(i - 1, max(-1, i - 4), -1):
            cand = texts[j]
            if cand and cand != me and not cand.startswith("http") and len(cand) < 40:
                author = cand.lstrip("@")
                break
        if not author:
            author = "unknown"
        key = author + "|" + t
        if key in seen:
            continue
        seen.add(key)
        comments.append({"author": author, "text": t})
    return comments[:12]


def open_profile():
    """Launch Instagram and land on Nova's profile tab."""
    _e.boot()
    if "com.instagram.android" not in _e.sh("pm list packages"):
        raise SystemExit("Instagram is not installed in the emulator")
    _e.sh("am force-stop com.instagram.android")
    _e.sh("monkey -p com.instagram.android -c android.intent.category.LAUNCHER 1")
    time.sleep(10)
    _e.onboarding()
    _e.ensure_account()
    # tap the profile tab to be sure we're on our grid
    ns = _e.nodes()
    tab = [x for x in ns if x["rid"].endswith("profile_tab") or x["desc"] == "Profile"]
    if tab:
        t = max(tab, key=lambda x: x["y"])
        _e.sh(f"input tap {t['x']} {t['y']}"); time.sleep(5)
    _e.onboarding()


def fetch_targets():
    """Nova's recent posts and their comments, via the emulator app."""
    open_profile()
    ns = _e.nodes()
    hrefs = _find_post_links(ns)
    if not hrefs:
        _e.sh("input swipe 540 1200 540 700 400"); time.sleep(3)
        hrefs = _find_post_links(_e.nodes())
    out = []
    me = config.NOVA_HANDLE
    for href in hrefs[:6]:
        _e.sh(f"am start -a android.intent.action.VIEW -d https://www.instagram.com{href}", timeout=30)
        time.sleep(6)
        _e.onboarding()
        ns = _e.nodes()
        comments = _find_comments(ns, me)
        out.append({"url": "https://www.instagram.com" + href,
                    "path": href, "comments": comments})
        _e.sh("input keyevent KEYCODE_BACK"); time.sleep(2)
    return out


def send_one(comment, post_path, reply_text, dry=False):
    """Open a post, tap Reply on `comment`, type `reply_text`, post it."""
    open_profile()
    _e.sh(f"am start -a android.intent.action.VIEW -d https://www.instagram.com{post_path}", timeout=30)
    time.sleep(6)
    _e.onboarding()
    # find the comment block by author+text, tap its Reply button
    want = comment["text"][:60]
    who = comment["author"]
    ns = _e.nodes()
    tapped = False
    for _ in range(6):
        ns = _e.nodes()
        _e.guard(ns)
        li = None
        for n in ns:
            blob = (n["text"] or "") + " " + (n["desc"] or "")
            if want in blob and who in blob:
                li = n; break
        if li:
            # look for a Reply button near this comment
            reply_btn = None
            for n in ns:
                if (n["text"] or "").strip().lower() == "reply" and abs(n["y"] - li["y"]) < 260:
                    reply_btn = n; break
            if reply_btn:
                _e.sh(f"input tap {reply_btn['x']} {reply_btn['y']}"); time.sleep(2)
                tapped = True
                break
        _e.sh("input swipe 540 1500 540 900 300"); time.sleep(2)
    if not tapped:
        _e.shot("emu_reply_not_found"); raise RuntimeError("comment not found to reply to")
    # type into the comment box
    box = _e.find(["Add a comment", "Reply", "Write a comment"])
    if not box:
        _e.shot("emu_no_comment_box"); raise RuntimeError("no comment box")
    _e.sh(f"input tap {box['x']} {box['y']}"); time.sleep(1.5)
    _e.sh("input keycombination 113 29; input keyevent KEYCODE_DEL")
    for i, line in enumerate(reply_text.splitlines()):
        if i:
            _e.sh("input keyevent KEYCODE_ENTER")
        if line.strip():
            _e.type_text(line.encode("ascii", "ignore").decode(), slow=True)
    if dry:
        _e.shot("emu_reply_dry")
        for _ in range(3):
            _e.sh("input keyevent KEYCODE_BACK"); time.sleep(1.5)
        return "dry"
    post_btn = _e.find(["Post"], exact=True)
    if not post_btn:
        _e.shot("emu_no_post_btn"); raise RuntimeError("no Post button")
    _e.sh(f"input tap {post_btn['x']} {post_btn['y']}"); time.sleep(5)
    _e.onboarding()
    return "replied"
