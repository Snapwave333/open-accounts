"""NVIDIA NIM: free text and vision model calls for captions and the quality judge."""
import base64
import json
import mimetypes
import re
import urllib.error
import urllib.request
from pathlib import Path

from . import config


class NimUnavailable(RuntimeError):
    pass


def _key() -> str:
    key = config.nvidia_key()
    if not key:
        raise NimUnavailable("NVIDIA_API_KEY is not set")
    return key


def _image_part(path: Path) -> dict:
    mime = mimetypes.guess_type(str(path))[0] or "image/jpeg"
    data = base64.b64encode(path.read_bytes()).decode("ascii")
    return {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{data}"}}


def chat(system: str, user: str, images: list[Path] = (), model: str | None = None,
         max_tokens: int = 1200, temperature: float = 0.4, timeout: int = 120) -> str:
    """One chat completion. Images are attached to the user turn. Raises on any failure."""
    content: list[dict] | str
    if images:
        content = [_image_part(Path(p)) for p in images] + [{"type": "text", "text": user}]
    else:
        content = user
    body = {
        "model": model or (config.NIM_VISION_MODEL if images else config.NIM_TEXT_MODEL),
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": content}],
        "max_tokens": max_tokens,
        "temperature": temperature,
    }
    req = urllib.request.Request(
        config.NIM_CHAT,
        data=json.dumps(body).encode("utf8"),
        headers={"Authorization": "Bearer " + _key(), "Content-Type": "application/json", "Accept": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            payload = json.load(r)
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf8", "replace")[:400]
        raise NimUnavailable(f"NIM HTTP {e.code}: {detail}") from e
    except urllib.error.URLError as e:
        raise NimUnavailable(f"NIM unreachable: {e.reason}") from e
    try:
        text = payload["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as e:
        raise NimUnavailable(f"NIM reply had no content: {json.dumps(payload)[:300]}") from e
    if not text or not text.strip():
        raise NimUnavailable("NIM returned an empty reply")
    return text.strip()


def chat_json(system: str, user: str, images: list[Path] = (), **kw) -> dict:
    """A completion that must be a JSON object; code fences and reasoning preambles are stripped."""
    text = chat(system + "\nReply with one JSON object and nothing else.", user, images, **kw)
    text = re.sub(r"<think>.*?</think>", "", text, flags=re.S)
    m = re.search(r"\{.*\}", text, flags=re.S)
    if not m:
        raise NimUnavailable(f"NIM reply was not JSON: {text[:200]}")
    return json.loads(m.group(0))
