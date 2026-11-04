"""Local image generation on the RTX 4070 through ComfyUI: SDXL (RealVisXL) with IP-Adapter
FaceID for identity consistency. Free. The server is started for a job and stopped after it so
the GPU goes back to the studio; the studio's voice server is stopped first when it holds the
VRAM (it starts on demand, so nothing restarts it).

Node graph (ComfyUI API format):
  CheckpointLoaderSimple -> [IPAdapterUnifiedLoaderFaceID -> IPAdapterFaceID] -> KSampler -> VAEDecode -> SaveImage
"""
import json
import os
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

from . import state

COMFY_ROOT = Path(os.environ.get("OPENACCOUNTS_COMFY_ROOT") or r"C:\Users\chrom\ComfyUI")
COMFY_PYTHON = Path(os.environ.get("OPENACCOUNTS_COMFY_PYTHON") or r"C:\Users\chrom\ComfyUI-venv\Scripts\python.exe")
COMFY_URL = "http://127.0.0.1:8188"
CHECKPOINT = "RealVisXL_V5.0_fp16.safetensors"
TTS_HEALTH = "http://127.0.0.1:17494/health"
STUDIO_RENDER_LOCK = Path(r"C:\Users\chrom\Desktop\StickExplainers\_logs\render.lock")

SIZES = {"4:5": (1024, 1280), "1:1": (1024, 1024), "3:4": (1024, 1368), "9:16": (1024, 1792), "16:9": (1792, 1024)}
STEPS = 35
CFG = 6.5
SAMPLER, SCHEDULER = "dpmpp_2m", "karras"
CREATE_NO_WINDOW = 0x0800_0000


class ComfyError(RuntimeError):
    pass


class NotInstalled(ComfyError):
    pass


# ---- server lifecycle --------------------------------------------------------

def _get(path: str, timeout: int = 10) -> dict:
    with urllib.request.urlopen(COMFY_URL + path, timeout=timeout) as r:
        return json.load(r)


def alive() -> bool:
    try:
        _get("/system_stats", timeout=3)
        return True
    except (urllib.error.URLError, OSError, ValueError):
        return False


def installed() -> list[str]:
    """What is missing for a run; empty means ready."""
    missing = []
    if not (COMFY_ROOT / "main.py").is_file():
        missing.append(f"ComfyUI at {COMFY_ROOT}")
    if not COMFY_PYTHON.is_file():
        missing.append(f"venv python at {COMFY_PYTHON}")
    for rel in (
        f"models/checkpoints/{CHECKPOINT}",
        "models/ipadapter/ip-adapter-faceid-plusv2_sdxl.bin",
        "models/loras/ip-adapter-faceid-plusv2_sdxl_lora.safetensors",
        "models/clip_vision/CLIP-ViT-H-14-laion2B-s32B-b79K.safetensors",
        "custom_nodes/ComfyUI_IPAdapter_plus/__init__.py",
    ):
        p = COMFY_ROOT / rel
        if not p.is_file() or p.stat().st_size < 1_000_000 and not rel.endswith("__init__.py"):
            missing.append(rel)
    return missing


def _tts_vram_gb() -> float:
    try:
        with urllib.request.urlopen(TTS_HEALTH, timeout=4) as r:
            return float(json.load(r).get("vram_gb") or 0.0)
    except (urllib.error.URLError, OSError, ValueError):
        return 0.0


def _studio_render_active() -> bool:
    """True when StickExplainers holds an active render lock (not a stale leftover)."""
    if not STUDIO_RENDER_LOCK.exists():
        return False
    try:
        age_s = time.time() - STUDIO_RENDER_LOCK.stat().st_mtime
    except OSError:
        age_s = 0.0

    def _make_running() -> bool:
        # Probe via a .ps1 file so the powershell child's CommandLine does not contain the
        # make.py needle (a -Command string would self-match and look like a live render).
        ps1 = STUDIO_RENDER_LOCK.parent / "_nova_make_probe.ps1"
        body = """$hits = Get-CimInstance Win32_Process | Where-Object {
  ($_.Name -eq 'python.exe' -or $_.Name -eq 'pythonw.exe') -and
  $_.CommandLine -like '*StickExplainers*make.py*'
}
if ($hits) { $hits[0].ProcessId }
"""
        try:
            ps1.write_text(body, encoding="utf-8")
            out = subprocess.run(
                ["powershell", "-NoProfile", "-File", str(ps1)],
                capture_output=True, text=True, timeout=30, creationflags=CREATE_NO_WINDOW,
            )
            return bool(out.stdout.strip())
        except (OSError, subprocess.TimeoutExpired):
            return True  # fail closed: assume busy

    if _make_running():
        return True

    # Empty/old lock with no make.py: treat as stale so Nova can use the free GPU.
    if age_s > 2 * 3600:
        try:
            STUDIO_RENDER_LOCK.unlink(missing_ok=True)
        except OSError:
            pass
        return False
    return True


def release_gpu() -> str | None:
    """Formerly stopped the studio voice server to free VRAM.

    HARD RULE (Eli): never kill make.py or the TTS/voice server. Nova must share the GPU
    with StickExplainers (ComfyUI --lowvram) or wait — never stop TTS here.
    """
    held = _tts_vram_gb()
    if held >= 1.0:
        state.log(
            f"comfy: TTS holds {held:.1f} GB; leaving it running (will not stop voice server)"
        )
    return None


class Server:
    """Starts local ComfyUI for one Session; stops it on exit so the GPU frees."""

    def __init__(self) -> None:
        self.proc: subprocess.Popen | None = None
        self._owned = False

    def __enter__(self) -> "Server":
        missing = installed()
        if missing:
            raise NotInstalled("missing: " + "; ".join(missing))

        # Wait out StickExplainers renders; never kill make.py / TTS.
        while _studio_render_active():
            state.log("comfy: waiting for StickExplainers render.lock / make.py to finish")
            time.sleep(20)

        release_gpu()  # no-op: TTS must stay up; share GPU via --lowvram below

        if alive():
            state.log("comfy: reusing already-running ComfyUI")
            return self

        cmd = [
            str(COMFY_PYTHON),
            str(COMFY_ROOT / "main.py"),
            "--listen", "127.0.0.1",
            "--port", "8188",
            "--lowvram",
        ]
        held = _tts_vram_gb()
        state.log(f"comfy: starting ComfyUI --lowvram (TTS VRAM ~{held:.1f} GB)")
        self.proc = subprocess.Popen(
            cmd,
            cwd=str(COMFY_ROOT),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=CREATE_NO_WINDOW,
        )
        self._owned = True
        deadline = time.time() + 240
        while time.time() < deadline:
            if self.proc.poll() is not None:
                raise ComfyError(f"ComfyUI exited early (code {self.proc.returncode})")
            if alive():
                state.log("comfy: ready")
                return self
            time.sleep(1.5)
        raise ComfyError("ComfyUI did not become ready within 240s")

    def __exit__(self, *exc) -> None:
        if not self._owned or self.proc is None:
            return None
        state.log("comfy: stopping ComfyUI")
        try:
            self.proc.terminate()
            self.proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                pass
        self.proc = None
        self._owned = False
        for _ in range(40):
            if not alive():
                break
            time.sleep(0.5)
        return None


def upload_image(path: Path) -> str:
    """Puts a reference image in ComfyUI's input folder; returns the name LoadImage expects."""
    boundary = "----oa" + uuid.uuid4().hex
    data = path.read_bytes()
    body = (
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{path.name}\"\r\n"
        f"Content-Type: image/jpeg\r\n\r\n"
    ).encode() + data + f"\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"overwrite\"\r\n\r\ntrue\r\n--{boundary}--\r\n".encode()
    req = urllib.request.Request(COMFY_URL + "/upload/image", data=body,
                                 headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)["name"]


def build_workflow(prompt: str, negative: str, aspect: str, seed: int, references: list[str], batch: int = 1,
                   faceid: bool = False) -> dict:
    """Build the Comfy graph. Default uses IP-Adapter PLUS (no InsightFace) so stylized locked
    faces still condition identity; set faceid=True for FaceID PLUS V2 when the ref is detectable."""
    w, h = SIZES[aspect]
    g = {
        "1": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": CHECKPOINT}},
        "2": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["1", 1], "text": prompt}},
        "3": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["1", 1], "text": negative}},
        "4": {"class_type": "EmptyLatentImage", "inputs": {"width": w, "height": h, "batch_size": batch}},
        "7": {"class_type": "VAEDecode", "inputs": {"samples": ["6", 0], "vae": ["1", 2]}},
        "8": {"class_type": "SaveImage", "inputs": {"images": ["7", 0], "filename_prefix": "nova"}},
    }
    model = ["1", 0]
    if references:
        prev = None
        for i, name in enumerate(references[:3]):
            nid = str(20 + i)
            g[nid] = {"class_type": "LoadImage", "inputs": {"image": name}}
            if prev is None:
                prev = [nid, 0]
            else:
                bid = str(30 + i)
                g[bid] = {"class_type": "ImageBatch", "inputs": {"image1": prev, "image2": [nid, 0]}}
                prev = [bid, 0]
        if faceid:
            g["10"] = {"class_type": "IPAdapterUnifiedLoaderFaceID",
                       "inputs": {"model": ["1", 0], "preset": "FACEID PLUS V2", "lora_strength": 0.6, "provider": "CPU"}}
            g["11"] = {"class_type": "IPAdapterFaceID",
                       "inputs": {"model": ["10", 0], "ipadapter": ["10", 1], "image": prev, "weight": 0.35,
                                  "weight_faceidv2": 0.45, "weight_type": "linear", "combine_embeds": "average",
                                  "start_at": 0.0, "end_at": 0.55, "embeds_scaling": "V only"}}
            model = ["11", 0]
        else:
            # CLIP-vision IP-Adapter: works when InsightFace cannot detect a stylized/pastel face.
            g["10"] = {"class_type": "IPAdapterUnifiedLoader",
                       "inputs": {"model": ["1", 0], "preset": "PLUS (high strength)"}}
            g["11"] = {"class_type": "IPAdapter",
                       "inputs": {"model": ["10", 0], "ipadapter": ["10", 1], "image": prev, "weight": 0.9,
                                  "weight_type": "linear", "combine_embeds": "average",
                                  "start_at": 0.0, "end_at": 0.75, "embeds_scaling": "V only"}}
            model = ["11", 0]
    g["6"] = {"class_type": "KSampler",
              "inputs": {"model": model, "positive": ["2", 0], "negative": ["3", 0], "latent_image": ["4", 0],
                         "seed": seed, "steps": STEPS, "cfg": CFG, "sampler_name": SAMPLER, "scheduler": SCHEDULER,
                         "denoise": 1.0}}
    return g


def run_workflow(graph: dict, timeout_s: int = 1800) -> list[dict]:
    """Queues the graph and returns the SaveImage outputs ({filename, subfolder, type})."""
    client = uuid.uuid4().hex
    req = urllib.request.Request(COMFY_URL + "/prompt", data=json.dumps({"prompt": graph, "client_id": client}).encode(),
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            queued = json.load(r)
    except urllib.error.HTTPError as e:
        raise ComfyError(f"ComfyUI rejected the workflow: {e.read().decode('utf8', 'replace')[:600]}") from e
    if queued.get("node_errors"):
        raise ComfyError(f"workflow errors: {json.dumps(queued['node_errors'])[:600]}")
    pid = queued["prompt_id"]
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        hist = _get(f"/history/{pid}", timeout=30).get(pid)
        if hist:
            status = hist.get("status", {})
            if status.get("status_str") == "error":
                msgs = [m for m in status.get("messages", []) if m[0] == "execution_error"]
                raise ComfyError(f"execution error: {json.dumps(msgs)[:600]}")
            images = []
            for node in hist.get("outputs", {}).values():
                images += node.get("images", [])
            if images:
                return images
        time.sleep(3)
    raise ComfyError(f"generation {pid} did not finish within {timeout_s}s")


def fetch(image: dict, dest: Path) -> Path:
    q = urllib.parse.urlencode({"filename": image["filename"], "subfolder": image.get("subfolder", ""), "type": image.get("type", "output")})
    dest.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(f"{COMFY_URL}/view?{q}", timeout=120) as r, dest.open("wb") as f:
        f.write(r.read())
    # SaveImage writes PNG; the feed wants JPEG and the hash/QC code expects it
    if dest.suffix.lower() == ".jpg":
        from PIL import Image

        with Image.open(dest) as im:
            im.convert("RGB").save(dest, "JPEG", quality=94)
    return dest



class Session:
    """One job's generations: the server is started on first use and stopped at close()."""

    def __init__(self) -> None:
        self.server: Server | None = None
        self.spent = 0.0  # always zero: local ComfyUI generation is free
        self._uploaded: dict[str, str] = {}

    def _ensure(self) -> None:
        if self.server is None:
            self.server = Server().__enter__()

    def close(self) -> None:
        if self.server is not None:
            self.server.__exit__(None, None, None)
            self.server = None

    def __enter__(self) -> "Session":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def images(self, prompt: str, negative: str, aspect: str, n: int, dest_dir: Path, stem: str,
               references: list[Path] = (), seed: int | None = None) -> list[Path]:
        self._ensure()
        names = []
        for ref in references:
            key = str(ref)
            if key not in self._uploaded:
                self._uploaded[key] = upload_image(ref)
            names.append(self._uploaded[key])
        seed = seed if seed is not None else int.from_bytes(os.urandom(4), "big")
        graph = build_workflow(prompt, negative, aspect, seed, names, batch=1, faceid=True)
        out: list[Path] = []
        for i in range(n):  # one at a time: a batch of SDXL latents does not fit beside --lowvram offloading
            graph["6"]["inputs"]["seed"] = seed + i
            t = time.time()
            produced = run_workflow(graph)
            path = fetch(produced[0], dest_dir / f"{stem}-{i + 1}.jpg")
            state.log(f"comfy: {path.name} in {time.time() - t:.0f}s (seed {seed + i}{', faceid' if names else ''})")
            out.append(path)
        return out
