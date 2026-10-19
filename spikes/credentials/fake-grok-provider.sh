#!/bin/sh
# Fake GROK_AUTH_PROVIDER_COMMAND: prints a fake token so `grok login` persists auth.json offline.
echo '{"access_token":"FAKE-AT-PROVIDER","refresh_token":"FAKE-RT","expires_in":1,"issuer":"https://idp.invalid"}'
