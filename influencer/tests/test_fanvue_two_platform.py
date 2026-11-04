"""Two-platform Fanvue tests: compliance engine, IP-safe parody, strategist calendar,
inventory dedupe and publisher gates. All offline and deterministic: no GPU, no network,
no NIM - the Fanvue API itself is exercised only by the live jobs, never here.

The whole suite runs against a throwaway data directory (OPENACCOUNTS_INFLUENCER_DATA is
set before any oa_influencer import), so the real pipeline data is never touched.
"""
import datetime as dt
import os
import sys
import tempfile
from pathlib import Path

_TMP = tempfile.mkdtemp(prefix="oa-fanvue-tests-")
os.environ["OPENACCOUNTS_INFLUENCER_DATA"] = _TMP

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from oa_influencer import character, config, fanvue_inventory, fanvue_policy, state  # noqa: E402
from oa_influencer.fanvue_strategist import FanvueContentStrategist, denver_wall, schedule_at  # noqa: E402

CLEAN_CAPTION = ("corset season never really ended\n"
                 "stay cursed \U0001F5A4\n\n"
                 "AI-generated character\n\n#cosplay #altgirl")


# ---- compliance engine ---------------------------------------------------------

def test_policy_loads_with_provenance():
    pol = fanvue_policy.load()
    assert len(pol["text_rules"]) >= 13
    assert set(pol["meta"]["docs"]) == {"CG", "AUP", "CT", "MP", "API", "CM", "CEP"}
    ids = {r["id"] for r in pol["text_rules"]}
    assert "minors_absolutely" in ids and "external_payments" in ids and "no_ai_denial" in ids


def test_clean_caption_passes():
    assert fanvue_policy.audit_text(CLEAN_CAPTION, "caption") == []
    assert fanvue_policy.check_caption(CLEAN_CAPTION, "sfw") == []


def test_missing_ai_disclosure_flagged():
    bad = "corset season\nstay cursed \U0001F5A4"
    violations = fanvue_policy.check_caption(bad, "sfw")
    assert any("missing_ai_disclosure" in v for v in violations)


def test_intimate_requires_18plus_marker():
    caption = "silk and secrets\nstay cursed \U0001F5A4\n\nAI-generated character"
    assert any("missing_18plus_marker" in v
               for v in fanvue_policy.check_caption(caption, "intimate"))
    ok = "18+ | silk and secrets\nstay cursed \U0001F5A4\n\nAI-generated character"
    assert fanvue_policy.check_caption(ok, "intimate") == []


def test_age_bait_blocked_everywhere():
    for text in ("just turned 18 energy", "barely legal vibes", "schoolgirl set"):
        assert fanvue_policy.audit_text(text, "caption"), text


def test_choker_allowed_choking_blocked():
    assert fanvue_policy.audit_text("velvet choker detail", "caption") == []
    assert any("extreme_themes" in v for v in fanvue_policy.audit_text("hand around her choking", "caption"))


def test_external_payments_and_migration_blocked():
    assert any("external_payments" in v for v in fanvue_policy.audit_text("tip me on cash.app", "message"))
    assert any("platform_migration" in v for v in fanvue_policy.audit_text("I'm also on onlyfans", "bio"))


def test_ai_denial_blocked_in_messages():
    assert any("no_ai_denial" in v for v in fanvue_policy.audit_text("i'm human i promise", "message"))


def test_ip_safe_names_blocked_but_looks_allowed():
    assert any("ip_safe_cosplay" in v for v in fanvue_policy.audit_text("my elsa set drops tonight", "caption"))
    assert any("ip_safe_cosplay" in v for v in fanvue_policy.audit_text("channeling Disney frost", "promo"))
    assert any("ip_safe_cosplay" in v for v in fanvue_policy.audit_text("velma dinkley glasses", "message"))
    assert fanvue_policy.audit_text("the ice queen said dress code: frost", "caption") == []
    assert fanvue_policy.audit_text("butterfly clips in her hair", "caption") == []


def test_weapons_blocked_on_prompt_surface():
    assert any("weapons_drugs" in v for v in fanvue_policy.audit_text("holding her minigun prop", "prompt"))


def test_pii_blocked_in_messages():
    assert any("pii_leak" in v for v in fanvue_policy.audit_text("text me 555-867-5309", "message"))


def test_fail_closed_without_policy(tmp_path):
    original = fanvue_policy.POLICY_JSON
    try:
        fanvue_policy.POLICY_JSON = tmp_path / "missing.json"
        violations = fanvue_policy.audit_text("perfectly innocent line", "caption")
        assert violations and "missing" in violations[0]
    finally:
        fanvue_policy.POLICY_JSON = original


def test_weapon_prop_stripped_for_fanvue():
    prompt = ("photorealistic editorial portrait, holding or with prop: chunky stylized prop minigun, "
              "neon alley")
    out, notes = fanvue_policy.adapt_prompt_for_fanvue(prompt, prop="chunky stylized prop minigun")
    assert "minigun" not in out and notes


# ---- IP-safe parody ------------------------------------------------------------

def test_name_patterns_cover_the_pool():
    patterns = character.fan_cosplay_name_patterns()
    joined = " | ".join(patterns)
    for expected in ("elsa", "disney", "velma dinkley", "aerith gainsborough", "zelda"):
        assert expected in joined, expected
    # generic words never banned
    for generic in ("star", "raven", "wednesday", "moon", "game", "games", "butterfly"):
        assert generic not in patterns, generic


def test_strip_character_names_removes_prefix_and_names():
    spec = character.spec()
    raw = "adult fan cosplay of Elsa: icy pale-blue gown, Disney frost, snowflake circlet"
    out = character.strip_character_names(raw, spec)
    lowered = out.lower()
    assert "elsa" not in lowered and "disney" not in lowered
    assert "adult fan cosplay" not in lowered
    assert "icy pale-blue gown" in out and "circlet" in out


def test_image_prompt_ip_safe_vs_named():
    spec = character.spec()
    pose = spec["poses"][0]
    theme_id = character.fan_theme_id("Elsa")  # pool theme: "fc:elsa"
    safe = character.image_prompt(spec, "famous_cosplays", theme_id, pose, ip_safe=True)
    named = character.image_prompt(spec, "famous_cosplays", theme_id, pose, ip_safe=False)
    low = safe["prompt"].lower()
    assert "elsa" not in low and "adult fan cosplay of" not in low
    # ip_safe appends the explicit no-trademark homage clause; plain mode does not
    assert "no trademarks" in low
    assert "no trademarks" not in named["prompt"].lower()
    assert safe["ip_safe"] is True and named["ip_safe"] is False


# ---- Denver scheduling ----------------------------------------------------------

def test_denver_dst_offsets():
    assert denver_wall(dt.datetime(2026, 7, 15, 18, 0, tzinfo=dt.timezone.utc)).hour == 12  # MDT -6
    assert denver_wall(dt.datetime(2026, 12, 15, 19, 0, tzinfo=dt.timezone.utc)).hour == 12  # MST -7
    summer = schedule_at(dt.date(2026, 7, 15), 12, 30)
    winter = schedule_at(dt.date(2026, 12, 15), 12, 30)
    assert summer.endswith("-06:00") and winter.endswith("-07:00")


# ---- strategist calendar ---------------------------------------------------------

def test_calendar_matches_strategy_spec():
    strat = FanvueContentStrategist()
    out = strat.build_calendar(today=dt.date(2026, 11, 4))
    assert out["total"] > 0
    cal = strat.load_calendar()
    slots = cal["slots"]
    feed = [s for s in slots if s["kind"] == "feed"]
    ppv = [s for s in slots if s["kind"] == "ppv"]

    # 30-day span, Sunday dark, 1-2 feed posts per posting day
    assert len(feed) >= 30 and len(feed) <= 61
    by_day: dict[str, int] = {}
    for s in feed:
        by_day[s["date"]] = by_day.get(s["date"], 0) + 1
    for date, count in by_day.items():
        weekday = dt.date.fromisoformat(date).weekday()
        if weekday == 6:
            assert count == 0, f"{date}: Sunday must stay dark"
        assert 1 <= count <= 2

    # 3-5 PPV releases per week (only weeks fully inside the 30-day window)
    ppv_weeks: dict[int, int] = {}
    start = dt.date(2026, 11, 4)
    horizon = start + dt.timedelta(days=30)
    for s in ppv:
        d = dt.date.fromisoformat(s["date"])
        week = d - dt.timedelta(days=d.weekday())
        if week >= start and week + dt.timedelta(days=6) <= horizon:
            key = week.isoformat()
            ppv_weeks[key] = ppv_weeks.get(key, 0) + 1
    assert ppv_weeks, "expected at least one fully covered week"
    for week, count in ppv_weeks.items():
        assert 3 <= count <= 5, f"week {week}: {count} PPV releases"

    # test windows honored (America/Denver wall clock)
    for s in slots:
        utc = dt.datetime.fromisoformat(s["scheduled_at"]).astimezone(dt.timezone.utc)
        wall = denver_wall(utc)
        if s["window"] == "midday":
            assert 11 <= wall.hour <= 13, s["id"]
        else:
            assert 18 <= wall.hour <= 21, s["id"]

    # shoots are 5-10 images; PPV carries a price; IP-safe rules apply to fan cosplay
    for s in feed:
        assert 5 <= s["images"] <= 10, s["id"]
        assert s["story"] and s["hashtags"]
    for s in ppv:
        assert 300 <= s["unlock_price_cents"] <= 50000
    famous = [s for s in slots if s["pillar"] == "famous_cosplays"]
    assert famous and all(s["ip_safe"] for s in famous)
    for s in famous:
        joined = " ".join(s["hashtags"]).lower()
        assert not any(p in joined for p in character.fan_cosplay_name_patterns())


def test_ppv_price_recommendation_in_range():
    strat = FanvueContentStrategist()
    low, high = config.FANVUE_PPV_PRICE_CENTS
    assert strat.recommend_ppv_price_cents(5) == low
    assert strat.recommend_ppv_price_cents(10) == high
    assert 300 <= strat.recommend_ppv_price_cents(7) <= high


# ---- inventory -----------------------------------------------------------------

def _make_image(path: Path, seed: int) -> Path:
    """Random-noise JPEG: two independent noise fields are always perceptually far apart
    (solid colors would share one phash and break the dedupe premise)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    import random

    from PIL import Image

    rng = random.Random(seed)
    Image.frombytes("RGB", (64, 64), bytes(rng.randrange(256) for _ in range(64 * 64 * 3))).save(path, "JPEG")
    return path


def test_inventory_register_dedupe_and_curation():
    from oa_influencer import fanvue_inventory as inv

    a = _make_image(Path(_TMP) / "shoot-a-1.jpg", seed=1)
    b = _make_image(Path(_TMP) / "shoot-a-2.jpg", seed=2)
    slot = {"id": "fanvue-test-feed-1", "pillar": "glam", "theme": "lace-under-blazer",
            "ip_safe": False, "assets": [
                {"id": "asset-1", "path": str(a), "approved": True},
                {"id": "asset-2", "path": str(b), "approved": True},
                {"id": "asset-3", "path": str(a), "approved": True},  # same file = same sha
            ]}
    assert inv.register_slot_assets(slot) == 2
    picked = inv.available(5)
    assert {p["id"] for p in picked} == {"asset-1", "asset-2"}
    inv.reserve(["asset-1"], "fanvue-test-ppv")
    assert [p["id"] for p in inv.available(5)] == ["asset-2"]
    inv.mark_posted(["asset-1"], "fanvue-test-ppv", status="posted")
    stats = inv.stats()
    assert stats["by_status"].get("posted") == 1
    # a missing file is flagged, never served
    c = _make_image(Path(_TMP) / "shoot-b-1.jpg", seed=3)
    c.unlink()
    inv.register_slot_assets({"id": "s2", "pillar": "glam", "theme": "x", "ip_safe": False,
                              "assets": [{"id": "asset-9", "path": str(c), "approved": True}]})
    assert all(Path(p["path"]).is_file() for p in inv.available(10))


# ---- publisher gates (no network: gate/spacing logic only) ------------------------

def test_publisher_policy_gate_blocks_bad_caption():
    from oa_influencer.fanvue_publisher import FanvuePublisher

    pub = FanvuePublisher(dry=True)
    slot = {"id": "s", "pillar": "cosplay", "theme": "vampire-countess", "assets": []}
    ok, violations = fanvue_policy.gate_slot(slot, "unlock on my cash.app \U0001F5A4\n\nAI-generated character")
    assert not ok and any("external_payments" in v for v in violations)
    clean = "the countess approves\nstay cursed \U0001F5A4\n\nAI-generated character"
    ok, violations = fanvue_policy.gate_slot(slot, clean)
    assert ok, violations


def test_publisher_gates_require_human_approval():
    from oa_influencer.fanvue_publisher import FanvuePublisher, NotAllowed

    pub = FanvuePublisher(dry=True)
    try:
        pub._gates()
        raise AssertionError("expected NotAllowed without gates")
    except NotAllowed as e:
        assert "account_created" in str(e) or "fanvue_publishing_authorized" in str(e)


def test_publisher_spacing_uses_fanvue_caps():
    from oa_influencer.fanvue_publisher import FanvuePublisher

    state.record_post({"at": state.iso(), "slot": "s1", "kind": "feed", "media": "x",
                       "handle": "nova", "platform": "fanvue", "result": "posted", "dry": False})
    pub = FanvuePublisher(dry=True)
    reason = pub._spacing_ok()
    assert reason and "minimum gap" in reason  # posted 0h ago: 4h spacing rule bites first
    state.record_post({"at": state.iso(), "slot": "s2", "kind": "feed", "media": "x",
                       "handle": "nova", "platform": "fanvue", "result": "posted", "dry": False})
    reason = pub._spacing_ok()
    assert reason and "daily cap" in reason
