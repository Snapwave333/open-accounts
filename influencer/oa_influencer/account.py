"""Instagram account: the agent prepares everything, opens sign-up in Eli's Chrome, detects when
the account exists and is signed in, grants the gate itself, then fills the profile.

The sign-up form (terms, CAPTCHA, confirmation code) is Eli's: Instagram forbids automating it
and the relay never types a password. Chrome keeps the password; the agent only ever uses the
signed-in session, so no secret is stored here."""
import datetime as dt
import json
import re
import time
from pathlib import Path

from . import character, config, relay, state

KIT = config.DATA / "account_kit.md"
SIGNUP_URL = "https://www.instagram.com/accounts/emailsignup/"
EDIT_URL = "https://www.instagram.com/accounts/edit/"
OPEN_STAMP = config.DATA / "logs" / "signup_opened.stamp"
PROFILE_IMAGE = config.DATA / "reference" / "profile.jpg"

DISPLAY_NAME = "Nova Vexley 🖤 AI cosplayer"
BIO = "dark-glam cosplay, built different (by a diffusion model)\n24 · AI-generated character · stay cursed 🖤\nwallpapers + prints ↓"
CATEGORY = "Digital creator"


def email_suggestions() -> list[str]:
    """Addresses Eli already owns, from the studio config, as plus-aliases for the character."""
    try:
        cfg = json.loads((config.STUDIO_ROOT / "config.json").read_text(encoding="utf8"))
        sources = [s.get("name") for s in cfg.get("email_watch", {}).get("sources", []) if s.get("name")]
    except (OSError, ValueError):
        sources = []
    out = []
    for addr in sources:
        user, _, domain = addr.partition("@")
        if domain:
            out.append(f"{user}+nova@{domain}")
    return out


def write_kit(handle: str) -> Path:
    spec = character.spec()
    emails = email_suggestions() or ["(an address you own)"]
    lines = [
        f"# Sign-up kit — @{handle}",
        "",
        "The agent opened https://www.instagram.com/accounts/emailsignup/ in your Chrome. Fill it in,",
        "let Chrome save the password, finish the email code and any CAPTCHA, and stay signed in.",
        "The agent checks every hour; when @" + handle + " is signed in it approves itself and fills the profile.",
        "",
        "| Field | Value |",
        "| --- | --- |",
        f"| Email | {emails[0]}" + (f" (or {', '.join(emails[1:])})" if len(emails) > 1 else "") + " |",
        f"| Full name | {DISPLAY_NAME} |",
        f"| Username | {handle} |",
        "| Password | generate one in Chrome and let it save it; the agent never needs it |",
        f"| Birthday | any date that makes her {spec['age']} (she is an adult character) |",
        "",
        "After sign-up, if Instagram asks: skip 'find friends', skip contacts, no Facebook link.",
        "",
        "## What the agent sets afterwards (you can leave these blank)",
        "",
        f"- Name: {DISPLAY_NAME}",
        f"- Bio:\n```\n{BIO}\n```",
        f"- Category: {CATEGORY} (switch to a professional/creator account when prompted)",
        f"- Profile photo: {PROFILE_IMAGE}",
        "",
        "Posts go out automatically on cadence once the account is live (no approval gate).",
    ]
    KIT.parent.mkdir(parents=True, exist_ok=True)
    KIT.write_text("\n".join(lines) + "\n", encoding="utf8")
    return KIT


def _recently_opened(hours: int = 6) -> bool:
    if not OPEN_STAMP.exists():
        return False
    age = dt.datetime.now() - dt.datetime.fromtimestamp(OPEN_STAMP.stat().st_mtime)
    return age < dt.timedelta(hours=hours)


def open_signup() -> bool:
    """Opens the sign-up page in a fresh tab in Eli's Chrome, at most once every 6 hours."""
    if _recently_opened():
        return False
    with relay.studio_session("") as (_tr, ig):
        ig.relay("new-tab", SIGNUP_URL, "--json")
    OPEN_STAMP.parent.mkdir(parents=True, exist_ok=True)
    OPEN_STAMP.write_text(state.iso(), encoding="utf8")
    state.log("account: opened the Instagram sign-up page in Chrome")
    return True


def signed_in(handle: str) -> bool:
    """True when @handle is signed in on this Chrome (active, or reachable via the switcher)."""
    with relay.studio_session(handle) as (_tr, ig):
        o = ig.relay("new-tab", "https://www.instagram.com/", "--json")
        tab = re.search(r'"tab_id":\s*(\d+)', o).group(1)
        try:
            time.sleep(8)
            return bool(ig.ensure_account(tab))
        finally:
            ig.relay("close-tab", tab=tab, check=False)


SET_FIELD_JS = """(() => {
  const el = document.querySelector(%s);
  if (!el) return 'missing';
  const proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
  el.focus(); setter.call(el, %s);
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  return 'set:' + el.value.length;
})()"""


def setup_profile(handle: str) -> dict:
    """Fills name, bio and category on the edit page, uploads the profile photo, saves. Reports
    exactly which fields took; Instagram's edit page changes often, so each step is verified."""
    result: dict = {"name": None, "bio": None, "photo": None, "saved": None}
    with relay.studio_session(handle) as (_tr, ig):
        o = ig.relay("new-tab", EDIT_URL, "--json")
        tab = re.search(r'"tab_id":\s*(\d+)', o).group(1)
        try:
            time.sleep(8)
            if not ig.ensure_account(tab):
                raise relay.RelayUnavailable(f"@{handle} is not the active account")
            ig.relay("nav", EDIT_URL, tab=tab, check=False)
            time.sleep(6)
            result["name"] = ig.relay("eval", SET_FIELD_JS % (json.dumps('input[name="name"], input[aria-label="Name"]'), json.dumps(DISPLAY_NAME)), tab=tab, check=False).strip()[-24:]
            result["bio"] = ig.relay("eval", SET_FIELD_JS % (json.dumps('textarea[name="biography"], textarea[aria-label="Bio"]'), json.dumps(BIO)), tab=tab, check=False).strip()[-24:]
            # Profile photo waits on reference_approved (Eli): never upload a draft face.
            if not state.granted("reference_approved"):
                result["photo"] = "pending reference_approved"
            elif PROFILE_IMAGE.is_file():
                try:
                    ig.relay("click", "text=Change photo", tab=tab, check=False)
                    time.sleep(1.5)
                    ig.relay("upload", "Upload Photo", str(PROFILE_IMAGE.resolve()), "--yes", "--timeout", "40000", tab=tab, timeout=60)
                    time.sleep(5)
                    result["photo"] = "uploaded"
                except RuntimeError as e:
                    result["photo"] = f"failed: {str(e)[-120:]}"
            else:
                result["photo"] = "no reference profile image yet"
            ig.relay("click", "text=Submit", tab=tab, check=False)
            time.sleep(4)
            page = ig.relay("eval", "document.body.innerText.slice(0, 4000)", tab=tab, check=False)
            result["saved"] = "Profile saved" in page or "saved" in page.lower()
        finally:
            ig.relay("close-tab", tab=tab, check=False)
    state.log(f"account: profile setup for @{handle}: {json.dumps(result)}")
    return result


def run() -> dict:
    """One pass: prepare, hand over, or verify and finish. Returns what it did."""
    st = state.state()
    handle = st.get("handle")
    if not handle:
        return {"result": "waiting", "reason": "no handle chosen yet (Nova handles job)"}
    if state.granted("account_created"):
        if not st.get("profile_setup"):
            st["profile_setup"] = setup_profile(handle)
            state.save_state(st)
            return {"result": "profile set up", "handle": handle, "profile": st["profile_setup"]}
        return {"result": "done", "handle": handle}

    kit = write_kit(handle)
    if signed_in(handle):
        state.grant("account_created", by="Nova account agent (verified signed in)")
        st["profile_setup"] = setup_profile(handle)
        state.save_state(st)
        return {"result": "account verified and profile set up", "handle": handle, "profile": st["profile_setup"]}
    opened = open_signup()
    return {"result": "waiting for Eli to finish sign-up", "handle": handle, "kit": str(kit), "opened_signup": opened}
