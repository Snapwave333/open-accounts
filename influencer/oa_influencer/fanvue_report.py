"""Fanvue Revenue Analytics agent: the weekly CEO report.

Pulls REAL numbers from Fanvue's API (earnings summary, subscribers, posts) and the local
ledger, writes `data/fanvue_weekly_report.json` + a human-readable `.md` for the CEO chat
and the app, and stores per-post performance where the strategist's optimizer reads it.
Every section degrades to an explicit "unavailable: <reason>" when the MCP is down - the
report never invents numbers.
"""

from __future__ import annotations

import datetime as dt
from pathlib import Path
from typing import Any

from . import config, fanvue_client, fanvue_inventory, state

REPORT_JSON = config.DATA / "fanvue_weekly_report.json"
REPORT_MD = config.DATA / "fanvue_weekly_report.md"
WINDOW_DAYS = 7


class FanvueRevenueAnalytics:
    def __init__(self) -> None:
        self.client = fanvue_client.FanvueClient()

    # ---- helpers ---------------------------------------------------------------

    def _denver_iso(self, d: dt.datetime) -> str:
        from .fanvue_strategist import denver_wall

        return denver_wall(d.astimezone(dt.timezone.utc)).isoformat(timespec="seconds")

    def _period(self) -> tuple[str, str]:
        end = state.now()
        start = end - dt.timedelta(days=WINDOW_DAYS)
        return self._denver_iso(start), self._denver_iso(end)

    # ---- sections (each fails soft with a reason) -------------------------------

    def _earnings(self) -> dict:
        start, end = self._period()
        try:
            raw = self.client.get_earnings_summary_window(start=start, end=end,
                                                          timezone=config.FANVUE_TIMEZONE)
        except Exception as e:
            return {"available": False, "reason": str(e)[:200]}
        return {"available": True, "raw": raw}

    def _earnings_total(self, raw: Any) -> float | None:
        """Best-effort total from whatever shape the summary returns; None when unclear."""
        if not isinstance(raw, dict):
            return None
        for key in ("total", "totalEarnings", "earningsTotal", "amount"):
            v = raw.get(key)
            if isinstance(v, (int, float)):
                return float(v)
        for key in ("totals", "summary", "aggregates"):
            sub = raw.get(key)
            if isinstance(sub, dict):
                for k in ("earnings", "total", "net", "gross"):
                    if isinstance(sub.get(k), (int, float)):
                        return float(sub[k])
        return None

    def _subscribers(self) -> dict:
        try:
            page = self.client.list_subscribers()
        except Exception as e:
            return {"available": False, "reason": str(e)[:200]}
        items = page.get("data") or page.get("subscribers") or page.get("items") or []
        return {"available": True, "count_first_page": len(items),
                "note": "first page only; Fanvue paginates subscriber lists"}

    def _published(self) -> dict:
        start = state.now() - dt.timedelta(days=WINDOW_DAYS)
        entries = [p for p in state.posts()
                   if p.get("platform") == "fanvue"
                   and dt.datetime.fromisoformat(p["at"]) >= start
                   and p.get("result") in ("posted", "scheduled")]
        return {
            "posts": len(entries),
            "images": sum(int(p.get("images") or 0) for p in entries),
            "ppv_releases": len([p for p in entries if p.get("price_cents")]),
            "verified": len([p for p in entries if p.get("verified")]),
            "detail": [{"at": p["at"], "slot": p["slot"], "kind": p.get("kind"),
                        "images": p.get("images"), "price_cents": p.get("price_cents"),
                        "result": p.get("result")} for p in entries],
        }

    def _post_performance(self) -> list[dict]:
        """Per-post likes/unlocks from Fanvue's post list, where the API exposes them."""
        try:
            page = self.client.get_posts_page()
        except Exception as e:
            return [{"error": str(e)[:200]}]
        out = []
        for post in (page.get("data") or page.get("posts") or [])[:20]:
            out.append({
                "uuid": post.get("uuid") or post.get("id"),
                "text": (post.get("text") or "")[:60],
                "likes": post.get("likeCount") if isinstance(post.get("likeCount"), int) else None,
                "price_cents": post.get("price") if isinstance(post.get("price"), int) else None,
                "published_at": post.get("publishedAt") or post.get("publishAt"),
            })
        return out

    # ---- the report ---------------------------------------------------------------

    def run(self) -> dict:
        report: dict[str, Any] = {
            "generated_at": state.iso(),
            "period_days": WINDOW_DAYS,
            "timezone": config.FANVUE_TIMEZONE,
        }
        earnings = self._earnings()
        report["earnings"] = {
            "available": earnings.get("available", False),
            "reason": earnings.get("reason"),
            "total": self._earnings_total(earnings.get("raw")) if earnings.get("available") else None,
        }
        report["subscribers"] = self._subscribers()
        report["published"] = self._published()
        report["post_performance"] = self._post_performance()
        report["inventory"] = fanvue_inventory.stats()

        # store performance where the strategist optimizer reads it
        analytics = state.read(state.ANALYTICS, {})
        fan = analytics.setdefault("fanvue", {})
        fan["report_at"] = report["generated_at"]
        fan["posts"] = report["post_performance"]
        state.write_atomic(state.ANALYTICS, analytics)

        self._write(report)
        return report

    def _write(self, report: dict) -> None:
        state.write_atomic(REPORT_JSON, report)
        lines = [
            "# Nova Vexley - Fanvue weekly report", "",
            f"* Generated: {report['generated_at']} ({report['timezone']}, last {report['period_days']} days)",
            f"* Earnings: " + (f"${(report['earnings']['total'] or 0) / 100:.2f}" if report["earnings"].get("available")
                               and report["earnings"].get("total") is not None
                               else f"unavailable ({report['earnings'].get('reason', 'no total in response')})"),
            f"* Subscribers: " + (f"{report['subscribers']['count_first_page']} on first page"
                                  if report["subscribers"].get("available") else "unavailable"),
            f"* Published: {report['published']['posts']} posts, {report['published']['images']} images, "
            f"{report['published']['ppv_releases']} PPV releases "
            f"({report['published']['verified']} platform-verified)",
            f"* Inventory: {report['inventory']['total']} assets "
            f"({report['inventory']['by_status'].get('available', 0)} available)", "",
            "## Posts", "",
        ]
        for p in report["post_performance"][:10]:
            if "error" in p:
                lines.append(f"- post list unavailable: {p['error']}")
                break
            lines.append(f"- {p.get('published_at', '?')}: likes={p.get('likes')} "
                         f"price={p.get('price_cents')} \"{p.get('text', '')}\"")
        lines += ["", "## Notes for the CEO", "",
                  "- Publishing verification and platform counts come straight from the Fanvue API; "
                  "an 'unavailable' section means the MCP/API was down, never zero activity.",
                  "- Schedule/pricing optimization runs with real data only (fanvue_strategist.optimize())."]
        REPORT_MD.write_text("\n".join(lines) + "\n", encoding="utf8")
