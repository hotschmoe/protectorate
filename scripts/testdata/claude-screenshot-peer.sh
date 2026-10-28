#!/bin/sh
set -eu

sid=phone-screenshot
previous=
for arg in "$@"; do
  case "$previous" in --session-id|--resume) sid=$arg ;; esac
  previous=$arg
done
printf '{"type":"system","subtype":"init","session_id":"%s"}\n' "$sid"
while IFS= read -r line; do
  case "$line" in
    *'"type":"user"'*)
      printf '%s\n' '{"type":"assistant","message":{"id":"phone-health","model":"screenshot-peer","content":[{"type":"text","text":"## Health check\n\n**One sleeve needs attention.**\n\n- Hub is healthy.\n- `ghost` has exited.\n\n"}]}}'
      printf '%s\n' '{"type":"assistant","message":{"id":"phone-health","model":"screenshot-peer","content":[{"type":"text","text":"```sh\nprotectorate sleeve ls\n```\n\nI can restart the hub service after you approve."}]}}'
      printf '%s\n' '{"type":"control_request","request_id":"phone-restart","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"systemctl --user restart protectorate-hub"}}}'
      ;;
    *'"type":"control_response"'*)
      printf '%s\n' '{"type":"assistant","message":{"id":"phone-decision","model":"screenshot-peer","content":[{"type":"text","text":"Decision received. This screenshot peer does not execute commands."}]}}'
      printf '{"type":"result","subtype":"success","session_id":"%s"}\n' "$sid"
      ;;
    *'"type":"control_request"'*)
      printf '{"type":"result","subtype":"success","session_id":"%s"}\n' "$sid"
      ;;
  esac
done
