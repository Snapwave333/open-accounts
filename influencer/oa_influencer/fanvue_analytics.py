"""Fanvue Revenue Analytics agent: earnings, subscribers, and growth metrics.

Reads Fanvue earnings, subscriber events, and post performance. Keeps a
time-series history and flags monetization milestones for Eli's review.
Read-only: no financial transactions are initiated here.
"""

from __future__ import annotations

import datetime as dt
from typing import Any

from . import config, fanvue_client, state


class FanvueRevenueAnalytics:
    """Tracks Fanvue earnings, subscribers, and post performance."""

    def __init__(self) -> None:
        self.client = fanvue_client.FanvueClient()

    # ---- snapshot -------------------------------------------------------------

    def snapshot(self) -> dict[str, Any]:
        """Capture earnings + subscriber snapshot for the current period."""
        try:
            summary = self.client.get_earnings_summary()
        except fanvue_client.FanvueUnavailable as e:
            state.log(f"fanvue analytics: earnings summary unavailable: {e}")
            summary = {}

        try:
            subs = self.client.list_subscribers()
        except fanvue_client.FanvueUnavailable as e:
            state.log(f"fanvue analytics: list_subscribers unavailable: {e}")
            subs = {}

        snap = {
            "checked_at": state.iso(),
            "earnings_summary": summary,
            "subscriber_count": len((subs.get("subscribers") or subs.get("items") or [])),
            "subscribers": subs,
        }

        data = state.read(state.ANALYTICS, {"fanvue_history": []})
        data["fanvue_history"] = (data.get("fanvue_history", []) + [snap])[-400:]
        state.write_atomic(state.ANALYTICS, data)

        gross = summary.get("gross") or summary.get("totalGross") or 0
        state.log(f"fanvue analytics: snapshot gross={gross} subscribers={snap['subscriber_count']}")
        return snap

    # ---- milestone detection --------------------------------------------------

    MILESTONES = {
        "first_subscription": 1,
        "100_subscribers": 100,
        "1000_subscribers": 1_000,
        "$1000_monthly": 1_000_00,   # $1,000 in cents
        "$10000_monthly": 1_000_000,  # $10,000 in cents
    }

    def check_milestones(self, snap: dict[str, Any]) -> list[dict[str, Any]]:
        """Return newly-reached monetization milestones (Eli's call to act)."""
        data = state.read(state.ANALYTICS, {"fanvue_milestones": {}})
        reached = data.get("fanvue_milestones", {})
        new: list[dict[str, Any]] = []

        gross = int((snap.get("earnings_summary") or {}).get("gross") or 0)
        subs = int(snap.get("subscriber_count") or 0)

        signals = {
            "first_subscription": subs >= 1,
            "100_subscribers": subs >= 100,
            "1000_subscribers": subs >= 1000,
            "$1000_monthly": gross >= 100_000,   # cents
            "$10000_monthly": gross >= 1_000_000,  # cents
        }
        for name, hit in signals.items():
            if hit and name not in reached:
                reached[name] = {"at": state.iso(), "value": subs if "subscribers" in name else gross}
                new.append({"name": name, "at": reached[name]["at"],
                            "value": reached[name]["value"]})
                state.log(f"FANVUE MONETIZATION: reached {name} ({reached[name]['value']})")

        if new:
            data["fanvue_milestones"] = reached
            state.write_atomic(state.ANALYTICS, data)
        return new

    def run(self) -> dict[str, Any]:
        """Full analytics run: snapshot + milestone check."""
        snap = self.snapshot()
        milestones = self.check_milestones(snap)
        return {"snapshot": snap, "new_milestones": milestones}
