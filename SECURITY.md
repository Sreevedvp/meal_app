# Security

This project is pre-1.0. Deploy only behind the authenticated gateway described in [the deployment guide](docs/DEPLOYMENT.md). Development mode has no account isolation and binds only to loopback.

Report vulnerabilities through the repository's **Security → Report a vulnerability** feature once the repository owner enables private vulnerability reporting. If that feature is unavailable, contact the repository owner privately; do not publish credentials, personal diet records or a working exploit in an issue. No private reporting address is configured in this source template.

The owner should enable dependency alerts, secret scanning, private vulnerability reports and required CI checks after creating the GitHub repository. Never commit `.env` files, database backups, APK signing keys or production logs. Rotate any secret that was committed; deleting the file is insufficient.

The current food prices and nutrition records are unverified fixtures. Allergen filtering validates these records only; it does not assess real products, package changes or cross-contact.
