#!/usr/bin/env python3
"""Create secrets in a new ignored, owner-readable file; never print them."""
import base64
import os
from pathlib import Path
import secrets

root = Path(__file__).resolve().parent.parent
text = (root / '.env.production.example').read_text()
values = {
    'PROXY_AUTH_SECRET': secrets.token_urlsafe(48),
    'OAUTH_COOKIE_SECRET': base64.urlsafe_b64encode(secrets.token_bytes(32)).decode(),
    'DB_ADMIN_PASSWORD': secrets.token_hex(32),
    'APP_DB_PASSWORD': secrets.token_hex(32),
}
lines = [f'{line.split("=", 1)[0]}={values[line.split("=", 1)[0]]}' if '=' in line and line.split('=', 1)[0] in values else line for line in text.splitlines()]
with os.fdopen(os.open(root / '.env.production', os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as output:
    output.write('\n'.join(lines) + '\n')
print('Created .env.production with private permissions. Fill in your domain and identity-provider settings before deployment.')
