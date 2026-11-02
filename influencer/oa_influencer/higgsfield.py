"""Higgsfield platform client: estimate, submit, poll, upload, download. Every paid call is
estimated first and refused when it would pass the daily or per-run budget."""
import json
import mimetypes
import time
import urllib.error
import urllib.request
from pathlib import Path

from . import config, state


class HiggsfieldError(RuntimeError):
    pass


class BudgetExceeded(HiggsfieldError):
    pass


class NotConfigured(HiggsfieldError):
    pass


def _auth() -> str:
    key = config.higgsfield_key()
    if not key:
        raise NotConfigured("HIGGSFIELD_API_KEY is not set (expected id:secret)")
    return "Key " + key


def _call(method: str, path: str, body: dict | None = None, timeout: int = 60) -> dict:
    url = config.HIGGSFIELD_API + path
    data = json.dumps(body).encode("utf8") if body is not None else None
    headers = {"Authorization": _auth(), "Accept": "application/json"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            raw = r.read()
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf8", "replace")[:500]
        if e.code == 401:
            raise NotConfigured(f"Higgsfield rejected the key (401): {detail}") from e
        if e.code == 403:
            raise BudgetExceeded(f"Higgsfield refused (403, usually no credits): {detail}") from e
        raise HiggsfieldError(f"Higgsfield {method} {path} -> {e.code}: {detail}") from e
    except urllib.error.URLError as e:
        raise HiggsfieldError(f"Higgsfield unreachable: {e.reason}") from e
    return json.loads(raw) if raw else {}


def estimate_usd(model: str, params: dict) -> float:
    """The platform's own price for this exact request, in USD."""
    out = _call("POST", f"/estimate/{model}", params)
    for key in ("usd", "price_usd", "cost_usd", "amount_usd"):
        if key in out:
            return float(out[key])
    raise HiggsfieldError(f"Estimate reply had no USD field: {json.dumps(out)[:300]}")


def check_budget(usd: float, run_spent: float) -> None:
    today = state.spent_today_usd()
    if today + usd > config.DAILY_USD_CAP:
        raise BudgetExceeded(
            f"Would spend ${usd:.3f}; ${today:.3f} already spent today of the ${config.DAILY_USD_CAP:.2f} daily cap"
        )
    if run_spent + usd > config.RUN_USD_CAP:
        raise BudgetExceeded(f"Would spend ${usd:.3f}; ${run_spent:.3f} already spent this run of the ${config.RUN_USD_CAP:.2f} run cap")


def submit(model: str, params: dict) -> dict:
    out = _call("POST", f"/{model}", params)
    if "request_id" not in out:
        raise HiggsfieldError(f"Submit reply had no request_id: {json.dumps(out)[:300]}")
    return out


def wait(request_id: str, timeout_s: int = 600, poll_s: float = 4.0) -> dict:
    """Polls until a terminal state. `nsfw` and `failed` raise; the caller logs and moves on."""
    deadline = time.time() + timeout_s
    while True:
        st = _call("GET", f"/requests/{request_id}/status")
        status = st.get("status")
        if status == "completed":
            return st
        if status in ("failed", "nsfw", "canceled"):
            raise HiggsfieldError(f"request {request_id} ended {status}: {st.get('error') or ''}".strip())
        if time.time() > deadline:
            raise HiggsfieldError(f"request {request_id} still {status} after {timeout_s}s")
        time.sleep(poll_s)


def download(url: str, dest: Path, timeout: int = 180) -> Path:
    dest.parent.mkdir(parents=True, exist_ok=True)
    req = urllib.request.Request(url, headers={"User-Agent": "openaccounts-influencer/1.0"})
    with urllib.request.urlopen(req, timeout=timeout) as r, dest.open("wb") as f:
        while chunk := r.read(1 << 16):
            f.write(chunk)
    if dest.stat().st_size == 0:
        dest.unlink()
        raise HiggsfieldError(f"Downloaded an empty file from {url}")
    return dest


def upload(path: Path) -> str:
    """Puts a local file in Higgsfield's temporary storage and returns its public URL."""
    content_type = mimetypes.guess_type(str(path))[0] or "application/octet-stream"
    grant = _call("POST", "/files/generate-upload-url", {"content_type": content_type})
    headers = dict(grant.get("upload_headers") or {"Content-Type": content_type})
    req = urllib.request.Request(grant["upload_url"], data=path.read_bytes(), headers=headers, method="PUT")
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            r.read()
    except urllib.error.HTTPError as e:
        raise HiggsfieldError(f"Upload of {path.name} failed: {e.code} {e.read().decode('utf8', 'replace')[:200]}") from e
    return grant["public_url"]


class Session:
    """Tracks what one pipeline run has spent so the per-run cap holds across calls."""

    def __init__(self) -> None:
        self.spent = 0.0

    def generate(self, model: str, params: dict, note: str, timeout_s: int = 600) -> dict:
        usd = estimate_usd(model, params)
        check_budget(usd, self.spent)
        queued = submit(model, params)
        rid = queued["request_id"]
        state.log(f"higgsfield: {model} queued {rid} (est ${usd:.3f}) {note}")
        try:
            result = wait(rid, timeout_s=timeout_s)
        except HiggsfieldError as e:
            # failed / nsfw requests are not charged by the platform
            state.log(f"higgsfield: {rid} {e}")
            raise
        self.spent += usd
        state.record_spend(model, usd, rid, note)
        result["estimated_usd"] = usd
        return result

    def images(self, prompt: str, aspect_ratio: str, n: int = 1, note: str = "",
               reference_id: str | None = None, resolution: str = "1080p") -> dict:
        params = {
            "prompt": prompt,
            "batch_size": n,
            "resolution": resolution,
            "aspect_ratio": aspect_ratio,
            "enhance_prompt": False,
        }
        if reference_id:
            params["custom_reference_id"] = reference_id
        return self.generate(config.SOUL_MODEL, params, note)

    def image_to_video(self, image_url: str, prompt: str, duration: int = 5, note: str = "") -> dict:
        params = {"prompt": prompt, "image_url": image_url, "duration": duration, "resolution": "1080p"}
        return self.generate(config.I2V_MODEL, params, note, timeout_s=900)
