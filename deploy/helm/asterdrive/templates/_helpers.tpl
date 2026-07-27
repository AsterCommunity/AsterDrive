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
{{- printf "%s-avatars" (include "asterdrive.fullname" .) }}
{{- end }}
{{- end }}

{{- define "asterdrive.image" -}}
{{- if .Values.image.digest }}
{{- printf "%s@%s" .Values.image.repository .Values.image.digest }}
{{- else }}
{{- printf "%s:%s" .Values.image.repository (.Values.image.tag | default .Chart.AppVersion) }}
{{- end }}
{{- end }}

{{- define "asterdrive.validateValues" -}}
{{- if and (not .Values.image.tag) (not .Values.image.digest) }}
{{- fail "set either image.tag or image.digest" }}
{{- end }}
{{- if and .Values.avatarPersistence.enabled (not .Values.avatarPersistence.create) (not .Values.avatarPersistence.existingClaim) }}
{{- fail "avatarPersistence requires create=true or a non-empty existingClaim when enabled" }}
{{- end }}
{{- if .Values.podDisruptionBudget.enabled }}
{{- $minSet := ne (toString .Values.podDisruptionBudget.minAvailable) "" }}
{{- $maxSet := ne (toString .Values.podDisruptionBudget.maxUnavailable) "" }}
{{- if eq $minSet $maxSet }}
{{- fail "podDisruptionBudget must set exactly one of minAvailable or maxUnavailable" }}
{{- end }}
{{- end }}
{{- $reservedConfigKeys := list
  "ASTER__DEPLOYMENT__PROFILE"
  "ASTER__SERVER__HOST"
  "ASTER__SERVER__PORT"
  "ASTER__SERVER__START_MODE"
  "ASTER__SERVER__TEMP_DIR"
  "ASTER__SERVER__UPLOAD_TEMP_DIR"
  "ASTER__CACHE__BACKEND"
  "ASTER__CONFIG_SYNC__BACKEND"
  "ASTER__CONFIG_SYNC__TOPIC"
  "ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES"
}}
{{- range $key := $reservedConfigKeys }}
{{- if hasKey $.Values.config.extra $key }}
{{- fail (printf "config.extra must not override chart-managed key %s" $key) }}
{{- end }}
{{- end }}
{{- end }}
