"""Growth Agent: Nova follows similar female cosplay creators to establish her profile.

Curated, human-approved seed list (Eli's direction, 2026-11-03: follow similar content female
cosplay creators) — never scraped auto-follows, never bulk: at most `GROW_FOLLOWS_PER_DAY` a
day, each one verified as a real public profile before the Follow click. Runs inside the
"Nova replies" agent on the runtime, so engagement (replies to her comments) and growth share
one Chrome session and journal.
"""
import json
import re
import time
from pathlib import Path

from . import config, state

STATE_FILE = config.DATA / "growth.json"

# Similar female cosplay / dark-glam creators (big-to-mid, public accounts). Nova is an
# AI-generated fan-cosplay profile: these are the peers whose audience overlaps her pillars.
SEEDS = [
    "jessicanigri",
    "yayahan",
    "alodiaalgosiengfiao",
    "jannetincosplay",
    "vantacreates",
    "honeyswordcosplay",
    "ridd1e",
    "kaypikefashion",
    "aurorakittty",
    "kamui_cosplay",
    "vesmedinia",
    "sabrinasousacosplay",
]

FOLLOW_CLICK = r"""(() => {
  const btn = [...document.querySelectorAll('button, div[role=button]')]
    .find(b => /^(Follow)$/i.test((b.innerText || '').trim()) && b.offsetParent);
  if (!btn) {
    const has = [...document.querySelectorAll('button, div[role=button]')]
      .some(b => /^(Following|Follow Back|Requested)$/i.test((b.innerText || '').trim()));
    return has ? 'already' : 'no-follow-button';
  }
  btn.click(); return 'clicked';
})()"""

PROFILE_CHECK = r"""(() => {
  const notFound = /page isn.t available|Sorry, this page/i.test(document.body.innerText.slice(0, 3000));
  const header = document.querySelector('header');
  return JSON.stringify({notFound, header: header ? header.innerText.slice(0, 220) : ''});
})()"""


def _load() -> dict:
    return state.read(STATE_FILE, {"followed": {}, "missing": [], "skipped": {}})


def _save(d: dict) -> None:
    state.write_atomic(STATE_FILE, d)


def followed_today(d: dict) -> int:
    today = state.now().date().isoformat()
    return len([1 for v in d["followed"].values() if v.get("at", "")[:10] == today])


def follow_one(tr, ig, tab: str, creator: str) -> str:
    """Open a creator's profile on Nova's Chrome and press Follow. Returns a status word."""
    o = tr.relay("new-tab", f"https://www.instagram.com/{creator}/", "--json")
    ctab = re.search(r'"tab_id":\s*(\d+)', o).group(1)
    try:
        time.sleep(7)
        raw = tr.relay("eval", PROFILE_CHECK, tab=ctab, check=False)
        m = re.search(r"\{.*\}", raw, re.S)
        info = json.loads(m.group(0)) if m else {}
        if info.get("notFound"):
            return "missing"
        r = tr.relay("eval", FOLLOW_CLICK, tab=ctab, check=False)
        return ("clicked" if "clicked" in r
                else "already" if "already" in r
                else "no-follow-button")
    finally:
        tr.relay("close-tab", tab=ctab, check=False)


def follow_one_emulator(creator: str) -> str:
    """Open a creator's profile in the emulator and press Follow. Returns a status word."""
    from . import emu_growth
    return emu_growth.follow_one(creator)


def run() -> dict:
    d = _load()
    created = state.granted("account_created")
    if not created or not state.state().get("handle"):
        return {"result": "waiting", "reason": "account"}
    if followed_today(d) >= config.GROW_FOLLOWS_PER_DAY:
        return {"result": "done", "reason": "daily follow cap", "follows_today": followed_today(d)}

    remaining = config.GROW_FOLLOWS_PER_DAY - followed_today(d)
    summary = {"followed": 0, "already": 0, "missing": 0, "no-button": 0}
    try:
        if config.INSTAGRAM_VIA == "emulator":
            from . import emu_growth
            consecutive_errors = 0
            for creator in SEEDS:
                if remaining <= 0:
                    break
                if creator in d["followed"] or creator in d["missing"]:
                    continue
                try:
                    status = emu_growth.follow_one(creator)
                    consecutive_errors = 0
                except Exception as e:
                    consecutive_errors += 1
                    state.log(f"growth: {creator}: emulator error {str(e)[:120]}")
                    if consecutive_errors >= 2:
                        state.log("growth: 2 emulator errors in a row; stopping this growth tick")
                        break
                    continue
                if status == "clicked":
                    d["followed"][creator] = {"at": state.iso()}
                    summary["followed"] += 1
                    remaining -= 1
                    state.log(f"growth: followed @{creator}")
                elif status == "already":
                    d["followed"][creator] = {"at": state.iso(), "already": True}
                    summary["already"] += 1
                elif status == "missing":
                    d["missing"].append(creator)
                    summary["missing"] += 1
                    state.log(f"growth: @{creator} not a live profile; skipped")
                else:
                    d["skipped"][creator] = d["skipped"].get(creator, 0) + 1
                    summary["no-button"] += 1
                    if d["skipped"][creator] >= 3:
                        d["missing"].append(creator)
                time.sleep(3)
        else:
            from . import relay
            handle = state.state()["handle"]
            with relay.studio_session(handle) as (tr, ig):
                o = tr.relay("new-tab", "https://www.instagram.com/", "--json")
                tab = re.search(r'"tab_id":\s*(\d+)', o).group(1)
                try:
                    time.sleep(6)
                    if not ig.ensure_account(tab):
                        state.log("growth: Nova Chrome not signed in; growth paused")
                        return {"result": "waiting", "reason": "not signed in"}
                finally:
                    tr.relay("close-tab", tab=tab, check=False)
                consecutive_errors = 0
                for creator in SEEDS:
                    if remaining <= 0:
                        break
                    if creator in d["followed"] or creator in d["missing"]:
                        continue
                    try:
                        status = follow_one(tr, ig, tab, creator)
                        consecutive_errors = 0
                    except Exception as e:
                        consecutive_errors += 1
                        state.log(f"growth: {creator}: relay error {str(e)[:120]}")
                        # circuit breaker: repeated relay failures can mean Instagram throttling —
                        # stop the tick entirely rather than look like a bot fighting a rate limit.
                        if consecutive_errors >= 2:
                            state.log("growth: 2 relay errors in a row; stopping this growth tick")
                            break
                        continue
                    if status == "clicked":
                        d["followed"][creator] = {"at": state.iso()}
                        summary["followed"] += 1
                        remaining -= 1
                        state.log(f"growth: followed @{creator}")
                    elif status == "already":
                        d["followed"][creator] = {"at": state.iso(), "already": True}
                        summary["already"] += 1
                    elif status == "missing":
                        d["missing"].append(creator)
                        summary["missing"] += 1
                        state.log(f"growth: @{creator} not a live profile; skipped")
                    else:
                        d["skipped"][creator] = d["skipped"].get(creator, 0) + 1
                        summary["no-button"] += 1
                        if d["skipped"][creator] >= 3:
                            d["missing"].append(creator)
                    time.sleep(3)
        _save(d)
        state.log(f"growth: {json.dumps(summary, ensure_ascii=False)}")
    except Exception as e:
        _save(d)
        state.log(f"growth: stopped ({str(e)[:160]})")
    return summary
