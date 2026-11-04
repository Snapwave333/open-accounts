"""Instagram posting for Nova (@nova_a_aa_) through the headless Pixel7 emulator (adb only).

Mirrors ig.share_media outcomes: "posted" | "dry". Posts only when the Instagram app in
the emulator is signed in as @nova_a_aa_ — never @snapwave. Cross-posting to Threads,
Facebook and Stories is kept off; AI label as requested; audience Everything=Everyone.

   from . import emu
   outcome = emu.post_image(path, caption, dry=dry, ai_label=True)  # "posted" | "dry"
"""
import re
import subprocess
import sys
import time
from io import BytesIO
from pathlib import Path

from . import config, state

if str(config.STUDIO_ROOT) not in sys.path:
    sys.path.insert(0, str(config.STUDIO_ROOT))
import tiktok_emu as e  # noqa: E402  # low-level adb helpers (ADB, SERIAL=emulator-5554, boot, nodes, tap, type_text...)

PKG = "com.instagram.android"
FEED = f"{PKG}/com.instagram.share.handleractivity.ShareHandlerActivity"
NOVA_HANDLE = config.NOVA_HANDLE
AI_LINE = "Animation and narration made with AI-assisted tools."
SHOTS = config.DATA / "logs" / "emu"
STOP_WORDS = ("captcha", "verify to continue", "drag the slider", "select all images",
              "too many attempts", "suspicious activity", "unusual activity",
              "agree and continue", "terms of service")


def adb(*a, check=False, timeout=60):
    r = subprocess.run([e.ADB, "-s", e.SERIAL, *a] if a and a[0] != "devices" else [e.ADB, *a],
                       capture_output=True, text=True, encoding="utf8", errors="replace", timeout=timeout)
    if check and r.returncode:
        raise RuntimeError(f"adb {' '.join(a)}: {r.stderr.strip()}")
    return r.stdout.strip()


def sh(cmd, timeout=60):
    return adb("shell", cmd, timeout=timeout)


def boot():
    """Boot (or reuse) the headless Pixel7 AVD (emulator-5554). Never spawns a duplicate."""
    return e.boot(window=False)


def up():
    return e.up()


def shot(name):
    SHOTS.mkdir(parents=True, exist_ok=True)
    p = SHOTS / f"{time.strftime('%H%M%S')}_{name}.png"
    p.write_bytes(subprocess.run([e.ADB, "-s", e.SERIAL, "exec-out", "screencap", "-p"],
                                 capture_output=True, timeout=60).stdout)
    return p


def nodes(tries=4):
    return e.nodes(tries)


def guard(ns):
    blob = " ".join((n["text"] + " " + n["desc"]).lower() for n in ns)
    for w in STOP_WORDS:
        if w in blob:
            shot("challenge")
            raise SystemExit(f"Instagram is showing a verification screen ('{w}'): stopping. Eli must clear it by hand.")


def find(labels, ns=None, exact=False):
    return e.find(labels, ns, exact)


def tap(labels, name, wait=15, exact=False, fallback=None):
    return e.tap(labels, name, wait, exact, fallback)


def _png():
    from PIL import Image
    raw = subprocess.run([e.ADB, "-s", e.SERIAL, "exec-out", "screencap", "-p"], capture_output=True, timeout=60).stdout
    return Image.open(BytesIO(raw)).convert("RGB")


def toggle_on(img, y, x=970):
    pts = [img.getpixel((xx, y)) for xx in range(905, 1036, 5)]
    lum = sum(0.3 * r + 0.59 * g + 0.11 * b for r, g, b in pts) / len(pts)
    return lum < 150


def set_toggle(label, want, x=970):
    n = e.find([label])
    if not n:
        return None
    for _ in range(3):
        if toggle_on(_png(), n["y"], x) == want:
            return True
        e.sh(f"input tap {x} {n['y']}"); time.sleep(1.5)
    return toggle_on(_png(), n["y"], x) == want


def onboarding():
    """One-time 'Set up on new device' / location screens: continue, decline location, never cross-post to Facebook."""
    for _ in range(4):
        ns = e.nodes()
        guard(ns)
        if e.find(["Share your reels to Facebook"], ns):
            d = e.find(["Not now"], ns, exact=True)
            if d:
                e.sh(f"input tap {d['x']} {d['y']}"); time.sleep(3); continue
        if e.find(["Set up on new device", "Location Services", "Others can now download"], ns):
            c = e.find(["Continue"], ns, exact=True)
            if c:
                e.sh(f"input tap {c['x']} {c['y']}"); time.sleep(3); continue
        d = e.find(["Don't allow", "Don't allow", "Not now", "Skip"], ns, exact=True)
        if d:
            e.sh(f"input tap {d['x']} {d['y']}"); time.sleep(3); continue
        return


def profile_posts():
    """Post count on our own profile (the reliable 'did it really post' check)."""
    ns = e.nodes()
    tab = [x for x in ns if x["rid"].endswith("profile_tab") or x["desc"] == "Profile"]
    if tab:
        t = max(tab, key=lambda x: x["y"])
        e.sh(f"input tap {t['x']} {t['y']}"); time.sleep(5)
    for x in e.nodes():
        m = re.match(r"^([\d,]+)\s*posts?$", x["text"] or x["desc"])
        if m:
            return int(m.group(1).replace(",", ""))
    return None


def active_account():
    """The username the app is signed in as (profile tab title), or None."""
    ns = e.nodes()
    tab = [x for x in ns if x["rid"].endswith("profile_tab") or x["desc"] == "Profile"]
    if tab:
        t = max(tab, key=lambda x: x["y"])
        e.sh(f"input tap {t['x']} {t['y']}"); time.sleep(5)
    n = next((x for x in e.nodes() if x["rid"].endswith("action_bar_title") and x["text"].strip()), None)
    return n["text"].strip() if n else None


def ensure_account():
    """Stop unless the app is signed in as @nova_a_aa_ (never posts to whoever is signed in)."""
    got = active_account()
    if got != NOVA_HANDLE:
        shot("ig_wrong_account")
        raise RuntimeError(f"the Instagram app is signed in as @{got}, not @{NOVA_HANDLE}: not posting")


def _type_caption(caption, labels):
    """Open the caption box, replace its text with `caption` (ASCII only), verify it landed."""
    box = e.find(labels)
    if not box:
        shot("ig_no_caption_box"); raise RuntimeError("no caption box")
    e.sh(f"input tap {box['x']} {box['y']}"); time.sleep(1.5)
    e.sh("input keycombination 113 29; input keyevent KEYCODE_DEL")
    for i, line in enumerate(caption.splitlines()):
        if i:
            e.sh("input keyevent KEYCODE_ENTER")
        if line.strip():
            e.type_text(line.encode("ascii", "ignore").decode(), slow=True)
    e.sh("input text %s"); time.sleep(1.5)
    typed = re.sub(r"\s+", " ", e.field_text()).strip()
    ok = e.find(["OK"], exact=True)
    if ok:
        e.sh(f"input tap {ok['x']} {ok['y']}"); time.sleep(2)
    if "mInputShown=true" in e.sh("dumpsys input_method | grep mInputShown"):
        e.sh("input keyevent KEYCODE_BACK"); time.sleep(1.5)
    want = re.sub(r"\s+", " ", caption.encode("ascii", "ignore").decode()).strip()
    if typed.lower() != want.lower():
        print("typed:", typed[:300])
        shot("ig_caption_missing"); raise RuntimeError("caption did not land")


def push_image(path):
    """Copy a card into the emulator's Pictures; returns its MediaStore id. Nova-only prefix."""
    path = Path(path)
    stem = re.sub(r"[^\w.-]+", "_", path.stem)[:60]
    remote = f"/sdcard/Pictures/nova_{stem}{path.suffix.lower()}"
    e.sh("rm -f /sdcard/Pictures/nova_*")
    e.adb("push", str(path), remote, check=True, timeout=120)
    e.sh(f"am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file://{remote}")
    for _ in range(20):
        m = re.search(r"_id=(\d+), _data=" + re.escape(remote.replace("/sdcard/", "/storage/emulated/0/")),
                      e.sh("content query --uri content://media/external/images/media --projection _id:_data"))
        if m:
            return m.group(1)
        time.sleep(1.5)
    raise RuntimeError("the media scanner did not index the image")


def post_image(path, caption, dry=False, ai_label=True):
    """One feed post from the app: kept at its own 4:5, caption, AI label as asked, no cross-posting,
    audience Everyone. Returns "posted" (profile post count went up) or "dry"."""
    boot()
    if PKG not in e.sh("pm list packages"):
        raise SystemExit("Instagram is not installed in the emulator")
    mid = push_image(path)
    for perm in ("READ_MEDIA_IMAGES", "POST_NOTIFICATIONS"):
        e.sh(f"pm grant {PKG} android.permission.{perm}")
    e.sh(f"am force-stop {PKG}")
    e.sh(f"monkey -p {PKG} -c android.intent.category.LAUNCHER 1")  # cold start drops the share intent
    time.sleep(10)
    onboarding()
    ensure_account()
    before = profile_posts()
    mime = "image/png" if str(path).lower().endswith(".png") else "image/jpeg"
    e.sh(f"am start -a android.intent.action.SEND -t {mime} --eu android.intent.extra.STREAM "
         f"content://media/external/images/media/{mid} --grant-read-uri-permission -n {FEED}")
    time.sleep(15)
    onboarding()
    shot("ig_feed_editor")
    e.tap(["Next"], "ig_feed_next", exact=True, wait=40)
    time.sleep(6)
    if e.find(["Sharing posts"]):
        e.tap(["OK"], "ig_feed_sheet", exact=True)
    onboarding()
    if not e.find(["New post"]):
        shot("ig_feed_lost"); raise RuntimeError("did not reach the New post screen")
    _type_caption(caption, ["Add a caption", "Write a caption"])
    e.sh("input swipe 540 1900 540 700 400"); time.sleep(2)
    ns = e.nodes()
    def row_value(label):
        n = e.find([label], ns)
        row = [x for x in ns if n and abs(x["y"] - n["y"]) < 40 and x["x"] > 500 and x["text"]]
        return row[0]["text"] if row else None
    if (row_value("Audience") or "Everyone") != "Everyone":
        raise RuntimeError(f"audience is {row_value('Audience')}, not Everyone")
    if (row_value("Also share on") or "Off") != "Off":
        shot("ig_feed_crosspost"); raise RuntimeError("cross-posting ('Also share on...') is not Off")
    if set_toggle("Add AI label", ai_label) is not True:
        shot("ig_feed_ai_label"); raise RuntimeError(f"could not set the AI label to {ai_label}")
    shot("ig_feed_before_share")
    if dry:
        print("dry run: stopped before Share")
        for _ in range(3):
            e.sh("input keyevent KEYCODE_BACK"); time.sleep(1.5)
            d = e.find(["Discard", "Discard post"], exact=True)
            if d:
                e.sh(f"input tap {d['x']} {d['y']}"); time.sleep(1.5)
        return "dry"
    e.tap(["Share"], "ig_feed_share", exact=True)
    time.sleep(20)
    onboarding()
    after = None
    for _ in range(12):
        after = profile_posts()
        if before is not None and after is not None and after > before:
            break
        e.sh(f"monkey -p {PKG} -c android.intent.category.LAUNCHER 1"); time.sleep(20)
    if before is None or after is None or after <= before:
        shot("ig_feed_not_posted")
        raise RuntimeError(f"post not on the profile (posts before {before}, after {after})")
    return "posted"
