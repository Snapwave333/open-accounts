"""Fanvue Publisher agent client.

Thin, authenticated wrapper around the Fanvue MCP tools used by the Nova Vexley
publisher/community/strategist/analytics agents. Every write action is guarded:
the agent chooses *what* to do; human approval is required for account-sensitive,
financial, unusual-messaging, custom-request, or irreversible operations.

MCP tool access: tools are reached through OpenCode's `tools.Fanvue[...]`
surface. This module only declares the thin client that callers use; the actual
transport is OpenCode's MCP connection (already authenticated via
`opencode mcp auth Fanvue`). When `tools` is not available (subprocess context),
every call raises FanvueUnavailable so callers degrade gracefully.
"""

from __future__ import annotations

import datetime as dt
import json
import os
from pathlib import Path
from typing import Any

from . import config, state


class FanvueUnavailable(RuntimeError):
    """The Fanvue MCP server is not reachable or authenticated."""


def _resolve_tool(name: str) -> Any:
    """Resolve an OpenCode MCP tool by path `tools.Fanvue["<name>"]`.

    Raises FanvueUnavailable when the tool is missing so callers degrade
    gracefully instead of crashing.
    """
    try:
        import builtins

        g = builtins.globals()
        tools = g.get("tools") if isinstance(g, dict) else None
    except Exception:
        tools = None
    if tools is None:
        raise FanvueUnavailable("OpenCode `tools` namespace not available")
    fanvue = getattr(tools, "Fanvue", None)
    if fanvue is None:
        raise FanvueUnavailable("Fanvue MCP namespace not available")
    tool = getattr(fanvue, name, None)
    if tool is None:
        raise FanvueUnavailable(f"Fanvue tool {name!r} not exposed by MCP server")
    return tool


class FanvueClient:
    """Authenticated Fanvue API client (OAuth via MCP)."""

    def __init__(self) -> None:
        self._verified = False
        self._creator_uuid: str | None = None
        self._scopes: set[str] = set()

    # ---- discovery / health ---------------------------------------------------

    def available(self) -> bool:
        """True when the Fanvue MCP namespace can be reached."""
        try:
            _resolve_tool("get-current-user")
            return True
        except FanvueUnavailable:
            return False

    def verify(self) -> dict[str, Any]:
        """Verify account authorization, endpoint availability, scopes.

        Returns a report dict the agents log. Raises FanvueUnavailable when
        the authenticated user is not a creator or the MCP call fails.
        """
        tool = _resolve_tool("get-current-user")
        try:
            me = tool({"X-Fanvue-API-Version": "2025-06-26"})
        except Exception as e:  # network / auth / MCP transport
            raise FanvueUnavailable(f"get-current-user failed: {e}") from e

        creator_uuid = (
            me.get("userUuid") or me.get("id") or me.get("uuid") or ""
        )
        handle = me.get("handle") or me.get("username") or ""
        scopes = set(me.get("scopes") or [])
        self._verified = bool(creator_uuid)
        self._creator_uuid = creator_uuid or None
        self._scopes = scopes

        state.log(f"fanvue: verified creator={creator_uuid} handle={handle} scopes={sorted(scopes)}")
        return {
            "creator_uuid": creator_uuid,
            "handle": handle,
            "scopes": sorted(scopes),
            "authorized": bool(creator_uuid),
        }

    # ---- post publishing (image upload 3-step) --------------------------------

    def start_image_upload(self, filename: str, mime_type: str = "image/jpeg") -> dict[str, Any]:
        """Step 1 of 3: opens an upload slot; returns mediaUuid/uploadId/uploadUrl."""
        tool = _resolve_tool("custom__start-image-upload")
        return tool({
            "X-Fanvue-API-Version": "2025-06-26",
            "filename": filename,
            "mimeType": mime_type,
        })

    def create_image_post(self, media_uuid: str, caption: str, **extra: Any) -> dict[str, Any]:
        """Step 3 of 3: completes upload + creates the post. Step 2 (PUT bytes
        to uploadUrl) is done by the caller before this."""
        tool = _resolve_tool("custom__create-image-post")
        payload: dict[str, Any] = {
            "X-Fanvue-API-Version": "2025-06-26",
            "mediaUuid": media_uuid,
            "caption": caption,
        }
        payload.update(extra)
        return tool(payload)

    def create_post(self, caption: str, **extra: Any) -> dict[str, Any]:
        """Fallback: create-creator-post (single endpoint, no upload URL dance)."""
        tool = _resolve_tool("create-creator-post")
        payload = {"X-Fanvue-API-Version": "2025-06-26", "caption": caption}
        payload.update(extra)
        return tool(payload)

    # ---- full media upload (multipart) + create-post ---------------------------
    # Used by the publisher for multi-image posts and PPV releases:
    # create-upload-session -> part URLs -> PUT bytes -> complete-upload-session,
    # then create-post with the collected mediaUuids.

    def create_media_session(self, name: str, filename: str, size_bytes: int,
                             media_type: str = "image") -> dict[str, Any]:
        tool = _resolve_tool("create-upload-session")
        return tool({
            "X-Fanvue-API-Version": "2025-06-26",
            "name": name,
            "filename": filename,
            "mediaType": media_type,
            "sizeBytes": size_bytes,
        })

    def get_upload_part_urls(self, upload_id: str, first: int, last: int) -> dict[str, Any]:
        tool = _resolve_tool("get-upload-part-urls")
        return tool({
            "X-Fanvue-API-Version": "2025-06-26",
            "uploadId": upload_id,
            "from": first,
            "to": last,
        })

    def complete_upload_session(self, upload_id: str, parts: list[dict[str, Any]]) -> dict[str, Any]:
        tool = _resolve_tool("complete-upload-session")
        return tool({
            "X-Fanvue-API-Version": "2025-06-26",
            "uploadId": upload_id,
            "parts": parts,
        })

    def upload_image(self, path: Path) -> str:
        """Uploads one image through the multipart session and returns its mediaUuid.
        Raises with the raw response excerpt when Fanvue's answer is not understood -
        the caller fails the upload rather than guessing."""
        data = path.read_bytes()
        session = self.create_media_session(path.stem, path.name, len(data))
        upload_id = session.get("uploadId") or session.get("upload_id")
        media_uuid = session.get("mediaUuid") or session.get("media_uuid")
        if not upload_id or not media_uuid:
            raise FanvueUnavailable(f"create-upload-session returned no uploadId/mediaUuid: {json.dumps(session)[:300]}")
        parts_meta = session.get("parts") or []
        if not parts_meta:
            total = int(session.get("totalParts") or 1)
            urls = self.get_upload_part_urls(upload_id, 1, total)
            parts_meta = urls.get("parts") or []
        completed: list[dict[str, Any]] = []
        for part in parts_meta:
            number = int(part.get("partNumber") or part.get("PartNumber") or 1)
            url = part.get("url") or part.get("uploadUrl")
            if not url:
                raise FanvueUnavailable(f"part {number} of {upload_id} has no presigned URL: {json.dumps(part)[:200]}")
            import urllib.request

            req = urllib.request.Request(url, data=data, method="PUT",
                                         headers={"Content-Type": "application/octet-stream"})
            with urllib.request.urlopen(req, timeout=300) as r:
                etag = r.headers.get("ETag") or r.headers.get("Etag") or ""
            completed.append({"PartNumber": number, "ETag": etag.strip('"')})
        done = self.complete_upload_session(upload_id, completed)
        final_uuid = (done or {}).get("mediaUuid") or media_uuid
        state.log(f"fanvue: uploaded {path.name} -> media {final_uuid}")
        return final_uuid

    def create_post_full(self, text: str, media_uuids: list[str], audience: str,
                         price: int | None = None, preview_uuid: str | None = None,
                         publish_at: str | None = None) -> dict[str, Any]:
        """create-post: multi-image post, optional PPV price + preview, optional scheduling."""
        tool = _resolve_tool("create-post")
        payload: dict[str, Any] = {
            "X-Fanvue-API-Version": "2025-06-26",
            "text": text,
            "mediaUuids": media_uuids,
            "audience": audience,
        }
        if price is not None:
            payload["price"] = int(price)
        if preview_uuid:
            payload["mediaPreviewUuid"] = preview_uuid
        if publish_at:
            payload["publishAt"] = publish_at
        return tool(payload)

    def update_subscription_price(self, cents: int, force_opt_in: bool = False) -> dict[str, Any]:
        tool = _resolve_tool("update-subscription-price")
        return tool({
            "X-Fanvue-API-Version": "2025-06-26",
            "subscriptionPrice": int(cents),
            "forceOptIn": bool(force_opt_in),
        })

    def get_earnings_summary_window(self, start: str | None = None, end: str | None = None,
                                    timezone: str = "America/Denver", granularity: str = "day") -> dict[str, Any]:
        tool = _resolve_tool("get-earnings-summary")
        payload: dict[str, Any] = {"X-Fanvue-API-Version": "2025-06-26",
                                   "timezone": timezone, "granularity": granularity}
        if start:
            payload["startDate"] = start
        if end:
            payload["endDate"] = end
        return tool(payload)

    def get_posts_page(self, cursor: str | None = None) -> dict[str, Any]:
        tool = _resolve_tool("get-posts")
        payload: dict[str, Any] = {"X-Fanvue-API-Version": "2025-06-26"}
        if cursor:
            payload["cursor"] = cursor
        return tool(payload)

    # ---- vault -----------------------------------------------------------------

    def list_media(self, cursor: str | None = None) -> dict[str, Any]:
        """Cursor-paginated list of media shared between creator and fans."""
        tool = _resolve_tool("list-media")
        payload = {"X-Fanvue-API-Version": "2025-06-26"}
        if cursor:
            payload["cursor"] = cursor
        return tool(payload)

    # ---- subscriptions / earnings / analytics ---------------------------------

    def get_earnings(self, cursor: str | None = None) -> dict[str, Any]:
        tool = _resolve_tool("get-earnings")
        payload = {"X-Fanvue-API-Version": "2025-06-26"}
        if cursor:
            payload["cursor"] = cursor
        return tool(payload)

    def get_earnings_summary(self) -> dict[str, Any]:
        tool = _resolve_tool("get-earnings-summary")
        return tool({"X-Fanvue-API-Version": "2025-06-26"})

    def list_subscribers(self, cursor: str | None = None) -> dict[str, Any]:
        tool = _resolve_tool("list-subscribers")
        payload = {"X-Fanvue-API-Version": "2025-06-26"}
        if cursor:
            payload["cursor"] = cursor
        return tool(payload)

    def get_subscriber(self, user_uuid: str) -> dict[str, Any]:
        tool = _resolve_tool("get-subscriber")
        return tool({"X-Fanvue-API-Version": "2025-06-26", "userUuid": user_uuid})

    # ---- messaging / community -------------------------------------------------

    def list_chats(self, filter_: str | None = None, cursor: str | None = None) -> dict[str, Any]:
        tool = _resolve_tool("list-chats")
        payload = {"X-Fanvue-API-Version": "2025-06-26"}
        if filter_:
            payload["filter"] = filter_
        if cursor:
            payload["cursor"] = cursor
        return tool(payload)

    def list_messages(self, user_uuid: str, cursor: str | None = None) -> dict[str, Any]:
        tool = _resolve_tool("list-messages")
        payload = {"X-Fanvue-API-Version": "2025-06-26", "userUuid": user_uuid}
        if cursor:
            payload["cursor"] = cursor
        return tool(payload)

    def send_message(self, user_uuid: str, text: str = "", **extra: Any) -> dict[str, Any]:
        tool = _resolve_tool("send-message")
        payload = {"X-Fanvue-API-Version": "2025-06-26", "userUuid": user_uuid}
        if text:
            payload["text"] = text
        payload.update(extra)
        return tool(payload)

    def send_mass_message(self, text: str, **extra: Any) -> dict[str, Any]:
        tool = _resolve_tool("send-mass-message")
        payload = {"X-Fanvue-API-Version": "2025-06-26", "text": text}
        payload.update(extra)
        return tool(payload)

    # ---- smart lists (fan segmentation) ----------------------------------------

    def get_smart_lists(self) -> dict[str, Any]:
        tool = _resolve_tool("get-smart-lists")
        return tool({"X-Fanvue-API-Version": "2025-06-26"})

    def get_smart_list_members(self, smart_list_uuid: str,
                               cursor: str | None = None) -> dict[str, Any]:
        tool = _resolve_tool("get-smart-list-members")
        payload = {"X-Fanvue-API-Version": "2025-06-26",
                   "smartListUuid": smart_list_uuid}
        if cursor:
            payload["cursor"] = cursor
        return tool(payload)

    # ---- read-only post inspection --------------------------------------------

    def get_posts(self, cursor: str | None = None) -> dict[str, Any]:
        tool = _resolve_tool("get-posts")
        payload = {"X-Fanvue-API-Version": "2025-06-26"}
        if cursor:
            payload["cursor"] = cursor
        return tool(payload)

    def get_post_by_uuid(self, post_uuid: str) -> dict[str, Any]:
        tool = _resolve_tool("get-creator-post-by-uuid")
        return tool({"X-Fanvue-API-Version": "2025-06-26",
                     "creatorUserUuid": self._creator_uuid or "",
                     "postUuid": post_uuid})

    # ---- state persistence ------------------------------------------------------

    def persist_verify_report(self, report: dict[str, Any]) -> None:
        data = state.read(state.ANALYTICS, {"fanvue": {}})
        data["fanvue"] = {
            **data.get("fanvue", {}),
            "verified_at": state.iso(),
            **report,
        }
        state.write_atomic(state.ANALYTICS, data)
