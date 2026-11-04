"""Engagement Agent: replies to comments on Nova's own Instagram posts, always in character.

Runs on the OpenAccounts autonomy runtime as `pipeline.py replies`. Discovers recent posts from
@<handle>'s profile (read-only), reads their comments, drafts a reply with the vision/text judge
in Nova's exact voice (character.json: tone, catchphrase sign-off, AI disclosure), and posts it
through Nova's Chrome profile. Safety mirrors the studio's comment rules: deterministic last
word before sending, daily caps, no spam/sexual/personal-info replies, never claim she is real,
never prices. Nothing is simulated: unreadable comments are skipped with a reason, and a reply
that did not show up after posting is logged as failed (never counted as sent).
"""
import datetime as dt
import json
import re
import time
import urllib.parse
from pathlib import Path

from . import character, config, nim, relay, state

STATE_FILE = config.DATA / "replies.json"
MIN_COMMENT_GAP = dt.timedelta(hours=1)   # do not double-send to the same comment
ONLY_FIRST_DAYS = 21                       # comments on posts older than this are ignored

REPLY_SYSTEM = """You reply to comments on Instagram for Nova Vexley, a fictional AI-generated adult
cosplay influencer who KNOWS she is AI and is playful about it. Project rules, follow exactly:
- Reply in Nova's voice: {tone}
- Available catchphrases when they fit: {catch}
- Every reply ends with the sign-off: {signoff}
- She never claims to be a real human, never mentions prices or asks anyone to buy anything,
  never talks to anyone about anything sexual, never gives advice to children.
- All-ages, no cruelty, no sarcasm aimed at the commenter, no hashtag soup, no mentions (@).
- 1-2 short sentences maximum."""

REPLY_USER = """Return JSON with exactly these keys:
- "category": one of "fan", "compliment", "question", "ai_curious", "spam", "toxic", "personal_info", "none"
- "reply_needed": true/false
- "reply": the in-character reply ("" when not needed)
Comment (author @{author}): {text}
Post theme: {theme}"""


IG_POSTS = r"""JSON.stringify([...new Set([...document.querySelectorAll('a[href*="/p/"], a[href*="/reel/"]')]
  .map(a => a.getAttribute('href')).filter(h => /^\/(?:[A-Za-z0-9._]+\/)?(?:p|reel)\/[\w-]+\/?$/.test(h)))].slice(0, 12))"""

# comments on our own post: user = the link's handle, text = the display text of the same block
IG_COMMENTS = r"""JSON.stringify((() => {
  const out = []; const me = %s;
  document.querySelectorAll('ul li, div[role=button] ~ div').forEach(li => {
    const a = li.querySelector('a[href^="/"][role=link], h3 a, span a[href^="/"]');
    const user = a ? a.getAttribute('href').replace(/\//g, '') : '';
    const span = [...li.querySelectorAll('span[dir=auto]')].map(s => s.innerText.trim()).filter(t => t && t !== user)[0];
    if (user && span && user !== me) out.push({user, text: span});
  });
  const seen = new Set();
  return out.filter(c => { const k = c.user + '|' + c.text; if (seen.has(k)) return false; seen.add(k); return true; });
})())"""

IG_REPLY = r"""(() => {
  const want = %s, who = %s;
  const li = [...document.querySelectorAll('ul li, div')].find(el => el.innerText && el.innerText.includes(want)
     && el.innerText.includes(who) && [...el.querySelectorAll('span, button, div[role=button]')].some(b => b.innerText.trim() === 'Reply')
     && el.innerText.length < 2000);
  if (!li) return 'no-comment';
  const btn = [...li.querySelectorAll('span, button, div[role=button]')].reverse().find(b => b.innerText.trim() === 'Reply');
  btn.click(); return 'reply-open';
})()"""

IG_TYPE = r"""(() => { const box = document.querySelector('textarea[aria-label^="Add a comment"], textarea');
  if (!box) return 'no-box'; box.focus(); document.execCommand('insertText', false, %s); return box.value; })()"""


# ---- state ------------------------------------------------------------------

def _load() -> dict:
    return state.read(STATE_FILE, {"seen": [], "done": {}, "posts": {}})


def _save(d: dict) -> None:
    state.write_atomic(STATE_FILE, d)


def replied_today(d: dict) -> int:
    today = state.now().date().isoformat()
    return len([v for v in d["done"].values() if v.get("at", "")[:10] == today])


# ---- in-character + policy gates (deterministic last word, like the studio's) ----------------

def safe(reply: str, theme: str = "") -> str | None:
    """A reason the reply must not be sent, or None when it is safe and in character."""
    r = reply.strip()
    if not r or len(r) < 3:
        return "empty reply"
    forbidden = [
        "nude", "naked", "topless", "nipple", "genital", "sex", "porn",  # compliance.nudity
        "child", "teen", "minor", "school", "kindergarten",             # all-ages
        "paypal", "onlyfans", "prices", "$", "€",                       # never sell in replies
        "http://", "https://", "#",                                     # no links/tags in replies
    ]
    low = r.lower()
    for w in forbidden:
        if w in low:
            return f"forbidden word: {w}"
    for w in character.spec()["compliance"]["forbidden_in_prompt"]:
        if w in low:
            return f"forbidden word: {w}"
    sign = character.spec()["voice"]["signoff"]
    if sign not in r:  # never let the model forget the sign-off
        r = f"{r.rstrip()} {sign}"
    if len(r) > 800:
        return "reply too long"
    return None


# ---- discovery --------------------------------------------------------------

def fetch_targets(session=None) -> list[dict]:
    """My recent posts and their comments. [{url, comments: [{author, text}]}]."""
    handle = state.state().get("handle")
    if not handle:
        return []
    if config.INSTAGRAM_VIA == "emulator":
        from . import emu_replies
        return emu_replies.fetch_targets()
    with relay.studio_session(handle) as (tr, ig):
        out: list[dict] = []
        o = tr.relay("new-tab", f"https://www.instagram.com/{handle}/", "--json")
        tab = re.search(r'"tab_id":\s*(\d+)', o).group(1)
        try:
            time.sleep(8)
            if not ig.ensure_account(tab):
                state.log(f"replies: @{handle} is not signed in; not reading comments")
                return out
            tr.relay("nav", f"https://www.instagram.com/{handle}/", tab=tab, check=False)  # pin the grid
            time.sleep(7)
            raw = tr.relay("eval", IG_POSTS, tab=tab, check=False)
            m = re.search(r"\[.*\]", raw, re.S)
            hrefs = json.loads(m.group(0)) if m else []
            if not hrefs:
                time.sleep(5)
                raw = tr.relay("eval", IG_POSTS, tab=tab, check=False)
                m = re.search(r"\[.*\]", raw, re.S)
                hrefs = json.loads(m.group(0)) if m else []
            for href in hrefs[:6]:
                tr.relay("nav", "https://www.instagram.com" + href, tab=tab, check=False)
                time.sleep(6)
                raw = tr.relay("eval", IG_COMMENTS % json.dumps(handle), tab=tab, check=False)
                m = re.search(r"\[.*\]", raw, re.S)
                comments = json.loads(m.group(0)) if m else []
                state.log(f"replies: {href}: {len(comments)} comment(s)")
                out.append({"url": "https://www.instagram.com" + href, "path": href,
                            "comments": comments[:12]})
        finally:
            tr.relay("close-tab", tab=tab, check=False)
        return out


def draft_in_character(author: str, text: str, theme: str) -> dict:
    """One judge call: classify and draft. Raises nim.NimUnavailable when NIM is down."""
    spec = character.spec()
    v = spec["voice"]
    prompt = REPLY_SYSTEM.format(tone=v["tone"], catch=", ".join(v["catchphrases"]), signoff=v["signoff"])
    return nim.chat_json(
        prompt,
        REPLY_USER.format(author=author, text=text[:300], theme=theme),
        max_tokens=config.NIM_REPLY_TOKENS,
        temperature=0.8,
    )


# ---- the send ---------------------------------------------------------------

def send_one(comment: dict, post_path: str, reply_text: str, dry: bool = False) -> str:
    """Reply in the comment's thread. Returns replied / dry / raises."""
    handle = state.state().get("handle")
    if config.INSTAGRAM_VIA == "emulator":
        from . import emu_replies
        return emu_replies.send_one(comment, post_path, reply_text, dry=dry)
    with relay.studio_session(handle) as (tr, ig):
        o = tr.relay("new-tab", "https://www.instagram.com" + post_path, "--json")
        tab = re.search(r'"tab_id":\s*(\d+)', o).group(1)
        try:
            time.sleep(8)
            if not ig.ensure_account(tab):
                raise RuntimeError(f"@{handle} is not signed in on this Chrome; not replying")
            r = tr.relay("eval", IG_REPLY % (json.dumps(comment["text"][:60]), json.dumps(comment["author"])),
                         tab=tab, check=False)
            if "reply-open" not in r:
                raise RuntimeError("comment not found on the post any more")
            time.sleep(1.5)
            typed = tr.relay("eval", IG_TYPE % json.dumps(reply_text), tab=tab, check=False)
            if reply_text[:20] not in typed:
                raise RuntimeError("reply text didn't land in the comment box")
            if dry:
                tr.relay("screenshot", str(config.DATA / "logs" / "replies_dry.png"), tab=tab, check=False)
                return "dry"
            tr.relay("click", "text=Post", tab=tab, check=False)
            time.sleep(5)
            body = tr.relay("eval", "document.body.innerText", tab=tab, check=False)
            if reply_text[:30] not in body:
                raise RuntimeError("reply didn't show up after posting (check the post)")
            return "replied"
        finally:
            tr.relay("close-tab", tab=tab, check=False)


# ---- the job ----------------------------------------------------------------

def run(dry: bool = False, limit: int = None) -> dict:
    d = _load()
    if not state.granted("account_created"):
        return {"result": "waiting", "reason": "account_created"}
    if not state.state().get("handle"):
        return {"result": "waiting", "reason": "no handle"}

    since = state.now() - dt.timedelta(days=ONLY_FIRST_DAYS)
    cap = config.REPLIES_PER_DAY_MAX - replied_today(d)
    summary = {"checked_comments": 0, "replied": 0, "dry": 0, "skipped": 0, "reasons": {}}
    if cap <= 0:
        summary["reasons"]["daily cap"] = 1
        return summary

    targets = fetch_targets(None)
    for post in targets:  # remember each post's age so very old comment threads stop being fetched
        d["posts"].setdefault(post["path"], {"first_seen": state.iso()})
    cid_key = lambda ctx: ctx["post"] + "|" + ctx["author"][:40] + "|" + ctx["text"][:80]
    work: list[dict] = []
    for post in targets:
        posted_at = d["posts"].get(post["path"], {}).get("first_seen")
        for c in post["comments"]:
            ctx = {"post": post["path"], "url": post["url"], "author": c["user"], "text": c["text"]}
            cid = cid_key(ctx)
            summary["checked_comments"] += 1
            if cid in d["done"] or cid in d["seen"]:
                continue
            d["seen"].append(cid)
            if posted_at and posted_at < since.isoformat():
                summary["skipped"] += 1; summary["reasons"]["old post"] = summary["reasons"].get("old post", 0) + 1
                continue
            if re.search(r"first|learn|follow|promo|free|gift|crypto|dm|whats app|whatsapp|hot link|bio link|mai n", c["text"], re.I):
                summary["skipped"] += 1; summary["reasons"]["spam-like"] = summary["reasons"].get("spam-like", 0) + 1
                continue
            work.append(ctx)
    d["seen"] = d["seen"][-3000:]
    _save(d)

    limit = limit if limit is not None else cap
    theme_default = "Nova's recent post on her profile"
    for ctx in work[:limit]:
        cid = cid_key(ctx)
        try:
            draft = draft_in_character(ctx["author"], ctx["text"], theme_default)
        except nim.NimUnavailable as e:
            # fail closed: pull it out of seen so the next run tries again
            d = _load(); d["seen"].remove(cid); _save(d)
            state.log(f"replies: NIM unavailable ({e}); comment left undealt")
            continue
        if not draft.get("reply_needed") or draft.get("category", "none") in ("spam", "toxic", "personal_info", "none"):
            d = _load()
            d["done"][cid] = {"post": ctx["post"], "author": ctx["author"], "text": ctx["text"][:200],
                              "category": draft.get("category"), "at": state.iso(), "status": "not-replied"}
            _save(d)
            summary["skipped"] += 1; summary["reasons"][draft.get("category", "none")] = summary["reasons"].get(draft.get("category", "none"), 0) + 1
            continue
        reply = draft["reply"].strip()
        why = safe(reply)
        if why:
            d = _load()
            d["done"][cid] = {"post": ctx["post"], "author": ctx["author"], "text": ctx["text"][:200],
                              "category": draft.get("category"), "at": state.iso(), "status": "blocked",
                              "why": why, "draft": reply[:300]}
            _save(d)
            state.log(f"replies: blocked ({why}): {reply[:80]!r}")
            summary["skipped"] += 1; summary["reasons"][why] = summary["reasons"].get(why, 0) + 1
            continue
        try:
            outcome = send_one(ctx, ctx["post"], reply, dry=dry)
        except Exception as e:
            d = _load()
            d["done"][cid] = {"post": ctx["post"], "author": ctx["author"], "text": ctx["text"][:200],
                              "at": state.iso(), "status": "failed", "why": str(e)[:200]}
            _save(d)
            state.log(f"replies: send failed on {ctx['post']} to @{ctx['author']}: {str(e)[:160]}")
            summary["reasons"]["send failed"] = summary["reasons"].get("send failed", 0) + 1
            continue
        d = _load()
        d["done"][cid] = {"post": ctx["post"], "author": ctx["author"], "text": ctx["text"][:200], "reply": reply,
                          "at": state.iso(), "status": outcome}
        _save(d)
        if outcome == "replied":
            summary["replied"] += 1
            state.log(f"replies: replied in character to @{ctx['author']} on {ctx['post']}: {reply[:80]!r}")
        else:
            summary["dry"] += 1
    state.log(f"replies: {json.dumps({k: v for k, v in summary.items() if k != 'reasons'})}"
              + (f" reasons={json.dumps(summary['reasons'])}" if summary["reasons"] else ""))
    return summary
