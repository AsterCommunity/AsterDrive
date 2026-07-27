{{- define "asterdrive.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "asterdrive.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{- define "asterdrive.fullnameWithSuffix" -}}
{{- $suffix := .suffix | trimPrefix "-" -}}
{{- $baseLength := sub 62 (len $suffix) | int -}}
{{- $base := include "asterdrive.fullname" .context | trunc $baseLength | trimSuffix "-" -}}
{{- printf "%s-%s" $base $suffix -}}
{{- end }}

{{- define "asterdrive.headlessName" -}}
{{- include "asterdrive.fullnameWithSuffix" (dict "context" . "suffix" "headless") -}}
{{- end }}

{{- define "asterdrive.clusterConfigName" -}}
{{- include "asterdrive.fullnameWithSuffix" (dict "context" . "suffix" "cluster") -}}
{{- end }}

{{- define "asterdrive.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "asterdrive.selectorLabels" -}}
app.kubernetes.io/name: {{ include "asterdrive.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "asterdrive.labels" -}}
helm.sh/chart: {{ include "asterdrive.chart" . }}
{{ include "asterdrive.selectorLabels" . }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "asterdrive.secretName" -}}
{{- required "existingSecret must name a Secret containing the AsterDrive cluster credentials" .Values.existingSecret }}
{{- end }}

{{- define "asterdrive.avatarClaimName" -}}
{{- if .Values.avatarPersistence.existingClaim }}
{{- .Values.avatarPersistence.existingClaim }}
{{- else }}
{{- include "asterdrive.fullnameWithSuffix" (dict "context" . "suffix" "avatars") }}
{{- end }}
{{- end }}

{{- define "asterdrive.image" -}}
{{- if .Values.image.digest }}
{{- printf "%s@%s" .Values.image.repository .Values.image.digest }}
{{- else }}
{{- printf "%s:%s" .Values.image.repository (.Values.image.tag | default .Chart.AppVersion) }}
{{- end }}
{{- end }}

{{- define "asterdrive.isSensitiveEnvKey" -}}
{{- $key := upper . -}}
{{- $sensitiveKeys := list
  "ASTER__DEPLOYMENT__INTERNAL_PROXY_SECRET"
  "ASTER__AUTH__JWT_SECRET"
  "ASTER__AUTH__SHARE_COOKIE_SECRET"
  "ASTER__AUTH__DIRECT_LINK_SECRET"
  "ASTER__AUTH__MFA_SECRET_KEY"
  "ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY"
  "ASTER__AUTH__WEBDAV_AUTH_CACHE_SECRET"
-}}
{{- if or
  (has $key $sensitiveKeys)
  (regexMatch "^ASTER__(DATABASE__URL|CACHE__ENDPOINT|CONFIG_SYNC__ENDPOINT)($|__)" $key)
-}}true{{- else -}}false{{- end -}}
{{- end }}

{{- define "asterdrive.validateValues" -}}
{{- if and (not .Values.image.tag) (not .Values.image.digest) }}
{{- fail "set either image.tag or image.digest" }}
{{- end }}
{{- if not .Values.avatarPersistence.enabled }}
{{- fail "cluster deployments require avatarPersistence.enabled=true because avatar files must be shared by every primary" }}
{{- end }}
{{- if and (not .Values.avatarPersistence.create) (not .Values.avatarPersistence.existingClaim) }}
{{- fail "avatarPersistence requires create=true or a non-empty existingClaim" }}
{{- end }}
{{- if .Values.podDisruptionBudget.enabled }}
{{- $minSet := ne (toString .Values.podDisruptionBudget.minAvailable) "" }}
{{- $maxSet := ne (toString .Values.podDisruptionBudget.maxUnavailable) "" }}
{{- if eq $minSet $maxSet }}
{{- fail "podDisruptionBudget must set exactly one of minAvailable or maxUnavailable" }}
{{- end }}
{{- end }}
{{- $chartManagedEnvKeys := list
  "POD_NAME"
  "POD_NAMESPACE"
  "ASTER__DEPLOYMENT__PROFILE"
  "ASTER__DEPLOYMENT__INTERNAL_ENDPOINT"
  "ASTER__DEPLOYMENT__INTERNAL_PROXY_SECRET"
  "ASTER__SERVER__HOST"
  "ASTER__SERVER__PORT"
  "ASTER__SERVER__START_MODE"
  "ASTER__SERVER__TEMP_DIR"
  "ASTER__SERVER__UPLOAD_TEMP_DIR"
  "ASTER__CACHE__BACKEND"
  "ASTER__CACHE__ENDPOINT"
  "ASTER__CACHE__ENDPOINT__BASE_URL"
  "ASTER__CACHE__ENDPOINT__USERNAME"
  "ASTER__CACHE__ENDPOINT__PASSWORD"
  "ASTER__CONFIG_SYNC__BACKEND"
  "ASTER__CONFIG_SYNC__ENDPOINT"
  "ASTER__CONFIG_SYNC__ENDPOINT__BASE_URL"
  "ASTER__CONFIG_SYNC__ENDPOINT__USERNAME"
  "ASTER__CONFIG_SYNC__ENDPOINT__PASSWORD"
  "ASTER__CONFIG_SYNC__TOPIC"
  "ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES"
  "ASTER__DATABASE__URL"
  "ASTER__AUTH__JWT_SECRET"
  "ASTER__AUTH__SHARE_COOKIE_SECRET"
  "ASTER__AUTH__DIRECT_LINK_SECRET"
  "ASTER__AUTH__MFA_SECRET_KEY"
  "ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY"
  "ASTER__AUTH__WEBDAV_AUTH_CACHE_SECRET"
}}
{{- range $key, $_ := .Values.config.extra }}
{{- if eq (include "asterdrive.isSensitiveEnvKey" $key) "true" }}
{{- fail (printf "config.extra must not contain sensitive key %s; provide it through existingSecret" $key) }}
{{- else if has (upper $key) $chartManagedEnvKeys }}
{{- fail (printf "config.extra must not override chart-managed key %s" $key) }}
{{- end }}
{{- end }}
{{- range $entry := .Values.extraEnv }}
{{- $name := get $entry "name" | default "" }}
{{- if eq (include "asterdrive.isSensitiveEnvKey" $name) "true" }}
{{- fail (printf "extraEnv must not define sensitive key %s; provide it through existingSecret" $name) }}
{{- else if has (upper $name) $chartManagedEnvKeys }}
{{- fail (printf "extraEnv must not override chart-managed key %s" $name) }}
{{- end }}
{{- end }}
{{- range $key := list "app.kubernetes.io/name" "app.kubernetes.io/instance" }}
{{- if hasKey $.Values.podLabels $key }}
{{- fail (printf "podLabels must not override StatefulSet selector label %s" $key) }}
{{- end }}
{{- end }}
{{- end }}
