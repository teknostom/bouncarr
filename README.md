# Bouncarr

An authentication proxy for the *arr stack (Sonarr, Radarr, Lidarr, etc.) that uses Jellyfin's authentication system for stateless access control.

## Features

- **Single Sign-On**: Use Jellyfin credentials for *arr applications
- **Zero State**: No database, no session storage - JWT-based authentication
- **Secure by Default**: Only Jellyfin administrators can access *arr applications
- **Lightweight**: Minimal resource usage, fast proxy performance
- **WebSocket Support**: Full support for real-time updates in *arr UIs

## Quick Start

### 1. Configuration

Copy the example configuration:

```bash
cp config.example.yaml config.yaml
```

Edit `config.yaml` with your settings:

```yaml
jellyfin:
  url: http://your-jellyfin:8096
  api_key: your_jellyfin_api_key

arr_apps:
  - name: sonarr
    url: http://your-sonarr:8989
  - name: radarr
    url: http://your-radarr:7878

server:
  host: 0.0.0.0
  port: 3000

security:
  access_token_expiry_hours: 24
  refresh_token_expiry_days: 30
  cookie_name: bouncarr_token
  refresh_cookie_name: bouncarr_refresh
  secure_cookies: false  # Set to true in production with HTTPS
```

### 2. Configure URL Base in *arr Applications

**IMPORTANT**: You must configure the URL Base in each *arr application:

**For Sonarr:**
1. Go to Settings → General → Host
2. Set **URL Base** to `/sonarr`
3. Save and restart Sonarr

**For Radarr:**
1. Go to Settings → General → Host
2. Set **URL Base** to `/radarr`
3. Save and restart Radarr

**For other *arr apps:** Follow the same pattern, setting URL Base to `/{app_name}` where `app_name` matches the name in your `config.yaml`.

> **Why is this needed?** The *arr apps generate links to resources (CSS, JS, images) using absolute paths. Without a URL Base, they'll request `/Content/styles.css` instead of `/sonarr/Content/styles.css`, causing 404 errors.

### 3. Run Bouncarr

```bash
cargo run --release
```

### 4. Access

Navigate to `http://localhost:3000/bouncarr/login` and log in with your Jellyfin administrator credentials.

Once logged in, access your *arr applications at:
- `http://localhost:3000/sonarr/`
- `http://localhost:3000/radarr/`
- etc.

## How It Works

1. **Login**: Users authenticate with Jellyfin credentials
2. **Authorization**: Only users with `isAdministrator` flag can proceed
3. **JWT Tokens**:
   - Access token expires at end of each day
   - Refresh token expires after 30 days (configurable)
   - JWT secret auto-generates on startup (restart to invalidate all sessions)
4. **Proxy**: All requests to configured *arr apps are proxied transparently
5. **WebSocket**: Real-time updates work seamlessly

## Architecture

```
User Browser → Bouncarr (this app) → Jellyfin (auth)
                   ↓
             *arr Applications
           (Sonarr, Radarr, etc.)
```

## Security

- **HTTP-only Cookies**: Prevents XSS attacks
- **Token Validation**: JWT tokens validated on each request
- **Admin-Only**: Only Jellyfin administrators can access
- **Stateless**: No session storage, tokens contain all info
- **Secret Rotation**: Restart server to invalidate all tokens

## Production Deployment

For production, run behind a reverse proxy (nginx, Traefik) with:
- TLS/HTTPS termination
- Proper domain name
- Set `secure_cookies: true` in config

## Development

```bash
# Run in development mode
cargo run

# Build release
cargo build --release

# Check code
cargo check

# Run tests
cargo test
```

## Configuration Details

### Jellyfin API Key

Get your Jellyfin API key from:
1. Jellyfin Dashboard → API Keys
2. Create a new API key
3. Add to `config.yaml`

### Adding More *arr Apps

Simply add more entries to the `arr_apps` list:

```yaml
arr_apps:
  - name: sonarr
    url: http://sonarr:8989
  - name: radarr
    url: http://radarr:7878
  - name: lidarr
    url: http://lidarr:8686
  - name: bazarr
    url: http://bazarr:6767
```

## Troubleshooting

### Login fails
- Check Jellyfin URL is correct and accessible
- Verify user has administrator privileges in Jellyfin
- Check logs for authentication errors

### Can't access *arr app
- Verify the *arr app URL in config
- Check that you're logged in (visit `/bouncarr/login`)
- Check browser console for errors

### WebSocket not working
- Ensure *arr app URL is accessible from Bouncarr
- Check that WebSocket endpoint path is correct

## License

MIT

## Contributing

Contributions welcome! Please open an issue or PR.
