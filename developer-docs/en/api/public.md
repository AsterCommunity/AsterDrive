# Public API

These paths are relative to `/api/v1` and do not require authentication.

Frontend bootstrap config, preview-app registry, thumbnail support, and media-data support are used by anonymous pages at startup. The remote-enrollment endpoints are used by the enrollment handshake between primary and follower nodes. These endpoints are registered only on `primary` nodes.

Public configuration endpoints include `Vary: Authorization, Cookie`. Anonymous responses usually use `Cache-Control: public, max-age=60`; `GET /public/custom-config` uses `Cache-Control: private, max-age=60` when the request carries a valid access token and authenticated-visible entries are included. Thumbnail-support and media-data-support responses are also cached in-process for 60 seconds and invalidated when media-processing config or storage policies change.

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/public/frontend-config` | Read public frontend bootstrap config |
| `GET` | `/public/preview-apps` | Read the anonymous-visible preview-app registry |
| `GET` | `/public/custom-config` | Read custom config entries visible to the current identity |
| `GET` | `/public/thumbnail-support` | Read public thumbnail extension support |
| `GET` | `/public/media-data-support` | Read public media metadata support |
| `POST` | `/public/remote-enrollment/redeem` | Follower redeems an enrollment token for remote-node binding information |
| `POST` | `/public/remote-enrollment/ack` | Follower confirms enrollment completion |

## `GET /public/frontend-config`

This endpoint is the current anonymous bootstrap entry for the frontend application. It returns the standard JSON envelope:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "version": 2,
    "branding": {
      "title": "AsterDrive",
      "description": "Self-hosted cloud storage",
      "favicon_url": "/favicon.svg",
      "wordmark_dark_url": "/static/asterdrive/asterdrive-dark.svg",
      "wordmark_light_url": "/static/asterdrive/asterdrive-light.svg",
      "site_urls": ["https://drive.example.com"],
      "allow_user_registration": true,
      "passkey_login_enabled": true
    },
    "downloads": {
      "archive_download_user_enabled": true,
      "archive_download_share_enabled": true
    },
    "media": {
      "image_preview_preference": "preview_first"
    }
  }
}
```

Key points:

- `version` is currently `2`
- `branding` is the only current public structure for brand and login-entry configuration; the old `/public/branding` route has been removed
- `downloads.archive_download_user_enabled` controls whether signed-in users can see and use ZIP package downloads in personal and team workspaces
- `downloads.archive_download_share_enabled` controls whether public-share visitors can use ZIP package downloads for shared content
- both fields come from backend runtime configuration. The frontend hides unavailable choices for usability, while archive-download endpoints enforce the switches again; UI visibility is not an authorization boundary
- `media.image_preview_preference` comes from runtime config `frontend_image_preview_preference`
- supported `image_preview_preference` values are `original_first` and `preview_first`
- the frontend caches this bootstrap config and refreshes it after related runtime config changes

## `GET /public/preview-apps`

Returns the public preview-app registry:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "version": 2,
    "apps": [
      {
        "key": "builtin.formatted",
        "provider": "builtin",
        "icon": "/static/preview-apps/json.svg",
        "enabled": true,
        "labels": {
          "en": "Formatted view",
          "zh": "格式化视图"
        },
        "extensions": ["json", "xml"]
      }
    ]
  }
}
```

Key points:

- `apps` contains the anonymous-visible previewer definitions
- supported providers currently include `builtin`, `url_template`, and `wopi`
- this is the v2 shape; matching information lives on each app's `extensions` and `config`
- disabled apps are filtered out
- frontend preview, public-share preview, and WOPI launch flows depend on this registry instead of hardcoding previewers in the frontend
- admins can currently maintain this registry through `/api/v1/admin/config/frontend_preview_apps_json`

## `GET /public/custom-config`

This endpoint returns the custom configuration visible to the current request identity, wrapped in the standard JSON envelope:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "entries": {
      "my-frontend.theme.primary_color": "#6366f1",
      "my-frontend.feature.enable_beta_tab": "true"
    }
  }
}
```

Notes:

- only `source = "custom"` entries are returned
- only the `entries` key/value map is exposed; internal admin fields such as `id`, `source`, and `updated_by` are not returned
- anonymous responses use `Cache-Control: public, max-age=60`
- when a valid access token is supplied and authenticated-visible entries are returned, the response uses `Cache-Control: private, max-age=60` with `Vary: Authorization, Cookie`
- visibility has three levels:
  - `private`: admin-only, never returned by this endpoint
  - `public`: readable without login
  - `authenticated`: returned only when the request carries a valid access token
- requests without a token only receive `public` entries
- requests that explicitly carry an invalid token return 401 instead of silently falling back to anonymous behavior
- this endpoint is intended for frontend-consumed configuration such as theme values, feature switches, and non-sensitive copy

## `GET /public/thumbnail-support`

Returns the server's public thumbnail-generation support:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "version": 1,
    "image_preview": {
      "enabled": true,
      "extensions": ["bmp", "gif", "jpeg", "jpg", "png", "webp"]
    },
    "image_thumbnail": {
      "enabled": true,
      "extensions": ["bmp", "gif", "jpeg", "jpg", "png", "webp"]
    },
    "audio_thumbnail": {
      "enabled": false
    },
    "video_thumbnail": {
      "enabled": false
    },
    "extensions": ["bmp", "gif", "jpe", "jpeg", "jpg", "png", "tif", "tiff", "webp"]
  }
}
```

Notes:

- extensions are normalized to lowercase without leading dots
- `image_preview`, `image_thumbnail`, `audio_thumbnail`, and `video_thumbnail` are the current per-use capability fields
- top-level `extensions` is kept as a compatibility union for older clients
- the built-in image processor exposes common image formats when enabled
- the built-in `lofty` processor can expose audio suffixes for embedded cover thumbnails
- `vips_cli` / `ffmpeg_cli` expose configured extensions only when the commands are available and the processors are enabled
- the capability mainly comes from `media_processing_registry_json`
- storage-native thumbnails and image previews can also contribute extensions when a storage policy opts in and the driver exposes that capability; built-in `tencent_cos` policies can expose it through COS CI, while built-in Local, S3-compatible, Azure Blob, OneDrive, and Remote policies do not

## `GET /public/media-data-support`

Returns media metadata parsing support:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "version": 1,
    "enabled": true,
    "max_source_bytes": 52428800,
    "kinds": {
      "image": {
        "enabled": true,
        "match": "extensions",
        "extensions": ["bmp", "gif", "jpeg", "jpg", "png", "tif", "tiff", "webp"]
      },
      "audio": {
        "enabled": true,
        "match": "extensions",
        "extensions": ["flac", "m4a", "mp3", "ogg", "wav"]
      },
      "video": {
        "enabled": false,
        "match": "extensions",
        "extensions": []
      }
    }
  }
}
```

The top-level `enabled` maps to `media_metadata_enabled`. The per-kind entries are derived from the active media-processing registry and bounded by `media_metadata_max_source_bytes`. Storage-native media metadata extensions can also be merged into audio/video support when a policy opts in and the driver exposes that capability; built-in `tencent_cos` policies can expose it through COS CI.

## `POST /public/remote-enrollment/redeem`

This endpoint is for the follower CLI enrollment flow, not anonymous browser clients.

Request:

```json
{
  "token": "enr_xxxxx"
}
```

Response:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "remote_node_id": 7,
    "remote_node_name": "edge-sh-01",
    "master_url": "https://drive.example.com",
    "access_key": "rk_xxx",
    "secret_key": "rs_xxx",
    "is_enabled": true,
    "ack_token": "enr_ack_xxx"
  }
}
```

`master_url` requires `public_site_url`; with multiple origins, the first one is used for enrollment. The access key and secret key are later used by the internal storage protocol.

## `POST /public/remote-enrollment/ack`

Request:

```json
{
  "ack_token": "enr_ack_xxx"
}
```

Success response:

```json
{
  "code": "success",
  "msg": ""
}
```

This means the follower has received the binding information and confirms that the enrollment session can end.
