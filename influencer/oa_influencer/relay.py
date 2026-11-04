"""Access to the studio's Chrome-relay helpers (`tiktok_relay`, `ig_relay`) for Nova's account.

Nova uses a dedicated Chrome user-data-dir (NovaChrome) + Chrome for Testing so --load-extension works (Google Chrome 137+ ignores it). Separate cookies from Default/@snapwave.
The OpenCode relay extension build in ~/.relay/opencoderealy/apps/chrome-extension-nova announces
browser_profile=nova; CLI calls pass --profile nova so Default and Nova bridges never mix tabs.

`studio_session(handle)` monkey-patches `ig.account` so ensure_account / post / stats all target
Nova only. StickExplainers CDN posts keep using config.json instagram_account on the Default profile.
A wrong-account result means Nova is not signed in on the Nova Chrome profile — never falls back
to the CDN handle. Re-check the active account before any submit.
"""
import contextlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

from . import config, state

RUN = "nova_instagram"
BROWSER_PROFILE = "nova"
NOVA_LOCK = Path(
    os.environ.get(
        "OPENACCOUNTS_NOVA_RELAY_LOCK",
        r"C:\Users\chrom\Desktop\StickExplainers\_logs\nova_instagram.lock",
    )
)
CHROME_EXE = Path(r"C:\Users\chrom\agent-tools\chrome-for-testing\chrome\win64-156.0.8078.80\chrome-win64\chrome.exe")  # Chrome for Testing: branded Chrome 137+ ignores --load-extension
NOVA_USER_DATA = Path(os.environ.get("OPENACCOUNTS_NOVA_CHROME_USER_DATA", str(Path(os.environ.get("LOCALAPPDATA", "")) / "NovaChrome")))
NOVA_EXTENSION = Path.home() / ".relay" / "opencoderealy" / "apps" / "chrome-extension-nova"
IG_LOGIN = "https://www.instagram.com/accounts/login/"
CREATE_NO_WINDOW = 0x08000000
DETACHED = 0x00000008


class RelayUnavailable(RuntimeError):
    pass


def _studio_modules():
    if not config.STUDIO_ROOT.is_dir():
        raise RelayUnavailable(f"studio not found at {config.STUDIO_ROOT}")
    if str(config.STUDIO_ROOT) not in sys.path:
        sys.path.insert(0, str(config.STUDIO_ROOT))
    import ig_relay  # noqa: WPS433
    import platform_utils  # noqa: WPS433
    import tiktok_relay as tr  # noqa: WPS433

    return tr, ig_relay, platform_utils


def ensure_nova_chrome() -> None:
    """Make sure a Chrome window for the Nova profile is up with the Nova relay extension."""
    if not CHROME_EXE.is_file():
        raise RelayUnavailable(f"Chrome not found at {CHROME_EXE}")
    if not NOVA_EXTENSION.is_dir():
        raise RelayUnavailable(f"Nova relay extension missing at {NOVA_EXTENSION}")
    try:
        tr, _ig, _pu = _studio_modules()
        prev_run, prev_relay = tr.RUN, tr.relay
        tr.RUN = RUN

        def _probe(*args, timeout=60, tab=None, check=True):
            tr.ensure_broker_and_chrome()
            cmd = ["node", str(tr.CLI), *args, "--run", tr.RUN, "--profile", BROWSER_PROFILE]
            if tab is not None:
                cmd += ["--tab", str(tab)]
            r = subprocess.run(
                cmd, capture_output=True, text=True, encoding="utf8", errors="replace", timeout=timeout + 30
            )
            return (r.stdout or "") + (r.stderr or "")

        tr.relay = _probe
        try:
            out = tr.relay("tabs", "--json", check=False)
        finally:
            tr.RUN = prev_run
            tr.relay = prev_relay
        if 'browser_profile": "nova"' in out or 'browser_profile":"nova"' in out:
            return
    except Exception:
        pass
    NOVA_USER_DATA.mkdir(parents=True, exist_ok=True)
    subprocess.Popen(
        [
            str(CHROME_EXE),
            f"--user-data-dir={NOVA_USER_DATA}",
            f"--load-extension={NOVA_EXTENSION}",
            "--no-first-run",
            "--no-default-browser-check",
            "--new-window",
            IG_LOGIN,
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=DETACHED,
    )
    time.sleep(3)


class _NovaSiteLock:
    """Cross-process lock for Nova Instagram only (does not block Default/@snapwave or TikTok)."""

    def __init__(self, wait=180):
        self.wait = wait
        self.f = None

    def __enter__(self):
        import msvcrt

        NOVA_LOCK.parent.mkdir(parents=True, exist_ok=True)
        self.f = open(NOVA_LOCK, "a+")
        end = time.time() + self.wait
        while True:
            try:
                self.f.seek(0)
                msvcrt.locking(self.f.fileno(), msvcrt.LK_NBLCK, 1)
                return self
            except OSError:
                if time.time() > end:
                    self.f.close()
                    raise TimeoutError("Nova Instagram relay lock busy")
                time.sleep(1)

    def __exit__(self, *exc):
        import msvcrt

        if not self.f:
            return
        try:
            self.f.seek(0)
            msvcrt.locking(self.f.fileno(), msvcrt.LK_UNLCK, 1)
        finally:
            self.f.close()


def _nova_relay(tr):
    def relay(*args, timeout=60, tab=None, check=True):
        tr.ensure_broker_and_chrome()
        cmd = ["node", str(tr.CLI), *args, "--run", tr.RUN, "--profile", BROWSER_PROFILE]
        if tab is not None:
            cmd += ["--tab", str(tab)]
        r = subprocess.run(
            cmd, capture_output=True, text=True, encoding="utf8", errors="replace", timeout=timeout + 30
        )
        out = (r.stdout or "") + (r.stderr or "")
        if check and (r.returncode != 0 or out.lstrip().startswith("\u274c")):
            raise RuntimeError(f"relay {args[0]} failed: {out.strip()[-300:]}")
        return out

    return relay


@contextlib.contextmanager
def studio_session(handle: str):
    """Nova Chrome profile + Nova-only lock. Does not take the CDN global pipeline_lock."""
    tr, ig, platform_utils = _studio_modules()
    ensure_nova_chrome()
    previous = (tr.RUN, ig.account, platform_utils.prep_for_platform, os.getcwd(), tr.relay)
    tr.RUN = RUN
    tr.relay = _nova_relay(tr)
    ig.account = lambda: handle
    platform_utils.prep_for_platform = lambda src, platform, base_caption="", long_url=None: (src, base_caption)
    os.chdir(config.STUDIO_ROOT)
    try:
        with _NovaSiteLock(wait=180):
            yield tr, ig
    finally:
        tr.RUN, ig.account, platform_utils.prep_for_platform = previous[:3]
        os.chdir(previous[3])
        tr.relay = previous[4]


POST_LINKS = r"""JSON.stringify([...new Set([...document.querySelectorAll('a[href*="/p/"], a[href*="/reel/"]')]
  .map(a => a.getAttribute('href')).filter(h => /^\/(?:[A-Za-z0-9._]+\/)?(?:p|reel)\/[\w-]+\/?$/.test(h)))].slice(0, 6))"""

POST_ENGAGEMENT = r"""(() => {
  const t = document.body.innerText;
  const likes = (t.match(/([\d.,]+[KM]?)\s+likes?/i) || [])[1] || null;
  const comments = (t.match(/([\d.,]+[KM]?)\s+comments?/i) || [])[1] || null;
  return JSON.stringify({likes, comments});
})()"""


def post_metrics(handle: str, limit: int = 3) -> list[dict]:
    """Like/comment counts on Nova's most recent posts (public page read). Unknown numbers are null."""
    out: list[dict] = []
    with studio_session(handle) as (tr, ig):
        o = ig.relay("new-tab", f"https://www.instagram.com/{handle}/", "--json")
        tab = re.search(r'"tab_id":\s*(\d+)', o).group(1)
        try:
            time.sleep(7)
            if not ig.ensure_account(tab):
                return out
            ig.relay("nav", f"https://www.instagram.com/{handle}/", tab=tab, check=False)  # pin the grid after account check
            time.sleep(7)
            raw = ig.relay("eval", POST_LINKS, tab=tab, check=False)
            m = re.search(r"\[.*\]", raw, re.S)
            hrefs = json.loads(m.group(0)) if m else []
            if not hrefs:  # grid still rendering: wait once more before reading posts
                time.sleep(5)
                raw = ig.relay("eval", POST_LINKS, tab=tab, check=False)
                m = re.search(r"\[.*\]", raw, re.S)
                hrefs = json.loads(m.group(0)) if m else []
            for href in hrefs[:limit]:
                ig.relay("nav", "https://www.instagram.com" + href, tab=tab, check=False)
                time.sleep(5)
                raw = ig.relay("eval", POST_ENGAGEMENT, tab=tab, check=False)
                m = re.search(r"\{.*\}", raw, re.S)
                vals = json.loads(m.group(0)) if m else {}
                out.append({"path": href, **vals, "read_at": state.iso()})
        finally:
            ig.relay("close-tab", tab=tab, check=False)
    return out


def profile_meta(handle: str) -> dict:
    """Public profile numbers via the meta description (read-only). None values mean unreadable."""
    with studio_session(handle) as (tr, ig):
        o = ig.relay("new-tab", f"https://www.instagram.com/{handle}/", "--json")
        tab = re.search(r'"tab_id":\s*(\d+)', o).group(1)
        try:
            time.sleep(7)
            meta = ig.relay(
                "eval",
                "document.querySelector('meta[name=description]')?.content || ''",
                tab=tab,
                check=False,
            )
            body = ig.relay("eval", "document.body.innerText.slice(0, 2000)", tab=tab, check=False)
        finally:
            ig.relay("close-tab", tab=tab, check=False)
    grab = lambda label: next((tr._num(x) for x in re.findall(r"([\d.,]+\s*[KMB]?)\s+" + label, meta, re.I)), None)
    unavailable = bool(re.search(r"Sorry, this page isn't available|page isn't available", body, re.I))
    return {
        "handle": handle,
        "exists": not unavailable and grab("Followers") is not None,
        "followers": grab("Followers"),
        "following": grab("Following"),
        "posts": grab("Posts"),
        "checked_at": state.iso(),
    }