"""Fanvue Community Manager agent: chats, fan segmentation, mass messages.

Handles inbound fan messages, replies in character, and manages smart-list
segments for targeted outreach. Human approval required for unusual messaging,
custom requests, and mass broadcasts.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from . import config, fanvue_client, nim, state


class NotAllowed(RuntimeError):
    pass


class FanvueCommunityManager:
    """Manages Fanvue fan relationships: chats, replies, smart-list segments."""

    def __init__(self, dry: bool = False) -> None:
        self.dry = dry
        self.client = fanvue_client.FanvueClient()

    # ---- gates ----------------------------------------------------------------

    def _gates(self, unusual: bool = False, mass: bool = False) -> None:
        if not state.granted("account_created"):
            raise NotAllowed("waiting for approval: account_created")
        if unusual and not state.granted("fanvue_unusual_messaging_authorized"):
            raise NotAllowed("waiting for approval: fanvue_unusual_messaging_authorized")
        if mass and not state.granted("fanvue_mass_messaging_authorized"):
            raise NotAllowed("waiting for approval: fanvue_mass_messaging_authorized")

    # ---- inbound --------------------------------------------------------------

    def list_unread_chats(self) -> list[dict[str, Any]]:
        """Return chats with unread messages, newest first."""
        try:
            data = self.client.list_chats(filter_="unread")
        except fanvue_client.FanvueUnavailable as e:
            state.log(f"fanvue community: list_chats unavailable: {e}")
            return []
        chats = data.get("chats") or data.get("items") or []
        return chats if isinstance(chats, list) else []

    def list_messages(self, user_uuid: str, limit: int = 20) -> list[dict[str, Any]]:
        try:
            data = self.client.list_messages(user_uuid)
        except fanvue_client.FanvueUnavailable as e:
            state.log(f"fanvue community: list_messages unavailable: {e}")
            return []
        msgs = data.get("messages") or data.get("items") or []
        return msgs if isinstance(msgs, list) else []

    # ---- reply in character ---------------------------------------------------

    REPLY_SYSTEM = """You are Nova Vexley, a fictional AI-generated 24-year-old dark-glam cosplayer
(@nova_a_aa_). Reply to a fan's message in character. Voice: warm, gentle,
slightly goth, confident but approachable. Never claim to be real. Never
mention prices, ask to buy, or share payment links. 1-2 short lines. End with
your signoff."""

    def _reply_text(self, fan_message: str, context: str = "") -> str:
        spec = character.spec()
        v = spec["voice"]
        system = self.REPLY_SYSTEM.format(signoff=v["signoff"])
        user = (f"Fan message: {fan_message}\n"
                f"Context: {context}\n"
                f"Write Nova's in-character reply (1-2 short lines, end with: "
                f"{v['signoff']}).")
        return nim.chat(system, user, temperature=0.8,
                        max_tokens=config.NIM_REPLY_TOKENS).strip().strip('"')

    def reply_to_chat(self, user_uuid: str, fan_message: str,
                      context: str = "", unusual: bool = False) -> dict[str, Any]:
        """Send an in-character reply to one fan. Human gate for unusual
        messaging; otherwise cadence-driven."""
        self._gates(unusual=unusual)

        text = self._reply_text(fan_message, context)

        if self.dry:
            state.log(f"fanvue community (dry): reply to {user_uuid}: {text[:80]}")
            return {"result": "dry", "user_uuid": user_uuid, "text": text}

        try:
            result = self.client.send_message(user_uuid, text=text)
        except fanvue_client.FanvueUnavailable as e:
            state.log(f"fanvue community: send_message unavailable: {e}")
            return {"result": "failed", "user_uuid": user_uuid, "reason": str(e)}
        except Exception as e:
            state.log(f"fanvue community: reply to {user_uuid} failed: {e}")
            return {"result": "failed", "user_uuid": user_uuid, "reason": str(e)}

        state.log(f"fanvue community: replied to {user_uuid}")
        return {"result": "sent", "user_uuid": user_uuid, "fanvue_message": result}

    # ---- mass broadcast (segmented) ------------------------------------------

    def run_mass_message(self, text: str, smart_list_uuid: str | None = None,
                         unusual: bool = False) -> dict[str, Any]:
        """Send a mass (broadcast) message to a smart list or all fans.
        Human gate required for mass broadcasts."""
        self._gates(mass=True, unusual=unusual)

        if self.dry:
            target = smart_list_uuid or "all fans"
            state.log(f"fanvue community (dry): mass message to {target}: {text[:80]}")
            return {"result": "dry", "target": target, "text": text}

        try:
            result = self.client.send_mass_message(text,
                                                   smartListUuid=smart_list_uuid)
        except fanvue_client.FanvueUnavailable as e:
            state.log(f"fanvue community: send-mass-message unavailable: {e}")
            return {"result": "failed", "reason": str(e)}
        except Exception as e:
            state.log(f"fanvue community: mass message failed: {e}")
            return {"result": "failed", "reason": str(e)}

        state.log("fanvue community: mass message sent")
        return {"result": "sent", "fanvue_message": result}

    # ---- smart lists / segmentation ------------------------------------------

    def list_smart_lists(self) -> list[dict[str, Any]]:
        try:
            data = self.client.get_smart_lists()
        except fanvue_client.FanvueUnavailable as e:
            state.log(f"fanvue community: get-smart-lists unavailable: {e}")
            return []
        lists = data.get("smartLists") or data.get("lists") or data.get("items") or []
        return lists if isinstance(lists, list) else []

    def list_segment_members(self, smart_list_uuid: str,
                             limit: int = 50) -> list[dict[str, Any]]:
        try:
            data = self.client.get_smart_list_members(smart_list_uuid)
        except fanvue_client.FanvueUnavailable as e:
            state.log(f"fanvue community: get-smart-list-members unavailable: {e}")
            return []
        members = data.get("members") or data.get("items") or []
        return members if isinstance(members, list) else []
