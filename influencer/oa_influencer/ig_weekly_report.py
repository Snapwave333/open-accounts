"""Weekly IG-to-Fanvue report: reach, profile visits, outbound clicks, Fanvue
conversions, subscriber acquisition, campaign performance, revenue attribution.

Mirrors fanvue_report.py: every section degrades to an explicit "unavailable:
<reason>" when a data source is down — the report never invents numbers.

Pulls from:
- data/analytics.json            Instagram reach / profile visits / recent posts (relay reads)
- data/ig_fanvue_clicks.json     outbound clicks to Fanvue (UTM-attributed)
- data/ig_fanvue_conversions.json paid conversions attributed to IG campaigns
- data/fanvue_weekly_report.json Fanvue earnings/subscribers (real API, if available)
- data/reels_calendar.json       Reel slots (themes, CTAs, campaign IDs)
- data/stories_calendar.json     Story slots (themes, CTAs, campaign IDs)
- data/calendar.json             Feed slots (themes, hashtags)

Writes data/ig_weekly_report.json + data/ig_weekly_report.md for the CEO chat/app.
"""
from __future__ import annotations

import datetime as dt
from pathlib import Path
from typing import Any

from . import analytics, config, ig_to_fanvue, state
from .fanvue_strategist import denver_wall

REPORT_JSON = config.DATA / "ig_weekly_report.json"
REPORT_MD = config.DATA / "ig_weekly_report.md"
WINDOW_DAYS = 7


class IGToFanvueWeeklyReport:
    def __init__(self) -> None:
        self.attribution = ig_to_fanvue.IGFanvueAttribution()

    # -- helpers ---------------------------------------------------------------

    def _denver_iso(self, d: dt.datetime) -> str:
        return denver_wall(d.astimezone(dt.timezone.utc)).isoformat(timespec="seconds")

    def _period(self) -> tuple[str, str]:
        end = state.now()
        start = end - dt.timedelta(days=WINDOW_DAYS)
        return self._denver_iso(start), self._denver_iso(end)

    def _window_start(self) -> dt.datetime:
        return state.now() - dt.timedelta(days=WINDOW_DAYS)

    # -- sections (each fails soft with a reason) ------------------------------

    def _ig_snapshot(self) -> dict[str, Any]:
        try:
            snap = analytics.snapshot()
        except Exception as e:
            return {"available": False, "reason": str(e)[:200]}
        return {"available": True, "raw": snap}

    def _recent_posts(self) -> dict[str, Any]:
        """Nova's Instagram posts in the window, with engagement read from relay stats."""
        start = self._window_start()
        entries = [
            p for p in state.posts()
            if p.get("platform") == "instagram"
            and dt.datetime.fromisoformat(p["at"]) >= start
        ]
        out = []
        for p in entries:
            out.append({
                "at": p["at"], "slot": p.get("slot"), "kind": p.get("kind"),
                "result": p.get("result"), "media": p.get("media"),
            })
        return {"posts": len(out), "detail": out}

    def _outbound_clicks(self) -> dict[str, Any]:
        clicks = self.attribution.total_clicks(days=WINDOW_DAYS)
        by_surface = self.attribution.surface_performance(days=WINDOW_DAYS)
        return {
            "clicks": clicks,
            "by_surface": by_surface,
        }

    def _conversions(self) -> dict[str, Any]:
        conv = self.attribution.total_conversions(days=WINDOW_DAYS)
        return conv

    def _fanvue(self) -> dict[str, Any]:
        """Fanvue earnings/subscribers from the Fanvue weekly report (real API)."""
        try:
            raw = state.read(state.ANALYTICS, {}).get("fanvue", {})
        except Exception:
            return {"available": False, "reason": "no fanvue analytics on disk"}
        if not raw:
            return {"available": False, "reason": "fanvue report not yet run"}
        report_path = config.DATA / "fanvue_weekly_report.json"
        if not report_path.exists():
            return {"available": False, "reason": "fanvue_weekly_report.json missing; run `pipeline.py fanvue-report`"}
        try:
            report = json.loads(report_path.read_text(encoding="utf8"))
        except Exception as e:
            return {"available": False, "reason": f"unreadable fanvue report: {e}"}
        return {
            "available": True,
            "earnings_total": report.get("earnings", {}).get("total"),
            "earnings_available": report.get("earnings", {}).get("available", False),
            "subscribers_first_page": report.get("subscribers", {}).get("count_first_page"),
            "subscribers_available": report.get("subscribers", {}).get("available", False),
            "published_posts": report.get("published", {}).get("posts", 0),
            "published_images": report.get("published", {}).get("images", 0),
            "ppv_releases": report.get("published", {}).get("ppv_releases", 0),
        }

    def _campaign_performance(self) -> list[dict[str, Any]]:
        return self.attribution.campaign_performance(days=WINDOW_DAYS)

    # -- the report ------------------------------------------------------------

    def run(self) -> dict[str, Any]:
        report: dict[str, Any] = {
            "generated_at": state.iso(),
            "period_days": WINDOW_DAYS,
            "timezone": "America/Denver",
        }

        ig = self._ig_snapshot()
        report["instagram"] = {
            "available": ig.get("available", False),
            "reason": ig.get("reason"),
            "followers": (ig.get("raw") or {}).get("followers"),
            "profile_visits": (ig.get("raw") or {}).get("profile_visits"),
            "reach": (ig.get("raw") or {}).get("reach"),
        }
        report["recent_posts"] = self._recent_posts()
        report["outbound_clicks"] = self._outbound_clicks()
        report["conversions"] = self._conversions()
        report["fanvue"] = self._fanvue()
        report["campaigns"] = self._campaign_performance()

        # store for dashboards / downstream consumption
        analytics_data = state.read(state.ANALYTICS, {})
        igf = analytics_data.setdefault("ig_to_fanvue", {})
        igf["report_at"] = report["generated_at"]
        igf["clicks"] = report["outbound_clicks"]["clicks"]
        igf["conversions"] = report["conversions"]
        igf["campaigns"] = report["campaigns"]
        state.write_atomic(state.ANALYTICS, analytics_data)

        self._write(report)
        return report

    def _write(self, report: dict[str, Any]) -> None:
        import json

        state.write_atomic(REPORT_JSON, report)
        ig = report["instagram"]
        clicks = report["outbound_clicks"]
        conv = report["conversions"]
        fv = report["fanvue"]

        lines = [
            "# Nova Vexley - IG-to-Fanvue weekly report", "",
            f"* Generated: {report['generated_at']} (America/Denver, last {report['period_days']} days)", "",
            "## Instagram", "",
            f"* Followers: {ig.get('followers') if ig.get('available') else 'unavailable (' + str(ig.get('reason', 'no data')) + ')'}",
            f"* Profile visits: {ig.get('profile_visits') if ig.get('available') else 'unavailable'}",
            f"* Reach: {ig.get('reach') if ig.get('available') else 'unavailable'}",
            f"* Posts this window: {report['recent_posts']['posts']}", "",
            "## Outbound clicks to Fanvue", "",
            f"* Total clicks: {clicks['clicks']}", "",
        ]
        for s in clicks["by_surface"]:
            lines.append(
                f"* {s['surface']}: {s['clicks']} clicks, "
                f"{s['conversions']} conversions, {s['revenue_usd']} USD "
                f"(CVR {s['conversion_rate']}%)"
            )
        lines += ["", "## Conversions (attributed to IG)", ""]
        lines.append(
            f"* Count: {conv['count']}  |  Revenue: ${conv['revenue_usd']:.2f}"
        )
        lines += ["## Fanvue (real API numbers)", ""]
        if fv.get("available"):
            lines.append(
                f"* Earnings: ${(fv.get('earnings_total') or 0) / 100:.2f} "
                f"({'available' if fv.get('earnings_available') else 'no total in response'})"
            )
            lines.append(
                f"* Subscribers: {fv.get('subscribers_first_page') if fv.get('subscribers_available') else 'unavailable'} on first page"
            )
            lines.append(
                f"* Published: {fv.get('published_posts')} posts, {fv.get('published_images')} images, "
                f"{fv.get('ppv_releases')} PPV releases"
            )
        else:
            lines.append(f"* unavailable ({fv.get('reason', 'no report')})")
        lines += ["", "## Campaign performance (clicks -> conversions)", ""]
        if report["campaigns"]:
            for c in report["campaigns"][:10]:
                lines.append(
                    f"- {c['campaign_id']} ({c['surface']}): {c['clicks']} clicks, "
                    f"{c['conversions']} conversions, ${c['revenue_usd']:.2f} revenue "
                    f"(CVR {c['conversion_rate']}%)"
                )
        else:
            lines.append("- no attributed campaigns yet")
        lines += ["", "## Recommendations", ""]
        lines.append(
            "- Posting times, formats, captions and CTAs are tuned from this report's "
            "click/conversion data; no change without >= 6 scored data points."
        )
        lines.append(
            "- Never purchase followers, fabricate engagement, or send unsolicited mass DMs."
        )
        lines.append(
            "- AI-generated disclosure stays on the bio, pinned post, Reel covers and fanvue highlight."
        )
        REPORT_MD.write_text("\n".join(lines) + "\n", encoding="utf8")


# module-level helper
_report: IGToFanvueWeeklyReport | None = None


def _get_report() -> IGToFanvueWeeklyReport:
    global _report
    if _report is None:
        _report = IGToFanvueWeeklyReport()
    return _report


def run() -> dict[str, Any]:
    return _get_report().run()
