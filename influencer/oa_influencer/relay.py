"""Access to the studio's Chrome-relay helpers (`tiktok_relay`, `ig_relay`) for Nova's account.
The studio code is imported in place and pointed at Nova's handle; nothing in the studio is edited."""
import contextlib
import os
import re
import sys
import time

from . import config, state

RUN = "nova_instagram"


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


@contextlib.contextmanager
def studio_session(handle: str):
    """The studio relay helpers, pointed at Nova's account for the duration, under the studio's own
    cross-process Chrome lock, with the working directory the studio scripts expect."""
    tr, ig, platform_utils = _studio_modules()
    if tr.other_driver_active():
        raise RelayUnavailable("another browser driver used Chrome in the last 5 minutes")
    previous = (tr.RUN, ig.account, platform_utils.prep_for_platform, os.getcwd())
    tr.RUN = RUN
    ig.account = lambda: handle
    # Nova's images are not studio renders: no watermark stripping, no studio caption rewrite.
    platform_utils.prep_for_platform = lambda src, platform, base_caption="", long_url=None: (src, base_caption)
    os.chdir(config.STUDIO_ROOT)
    try:
        with tr.pipeline_lock(wait=120):
            yield tr, ig
    finally:
        tr.RUN, ig.account, platform_utils.prep_for_platform = previous[:3]
        os.chdir(previous[3])


def profile_meta(handle: str) -> dict:
    """Public profile numbers via the meta description (read-only). None values mean unreadable."""
    with studio_session(handle) as (tr, ig):
        o = ig.relay("new-tab", f"https://www.instagram.com/{handle}/", "--json")
        tab = re.search(r'"tab_id":\s*(\d+)', o).group(1)
        try:
            time.sleep(7)
            meta = ig.relay("eval", "document.querySelector('meta[name=description]')?.content || ''", tab=tab, check=False)
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
