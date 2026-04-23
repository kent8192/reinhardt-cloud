{{/*
Expand the name of the chart.
*/}}
{{- define "reinhardt-cloud-agent.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "reinhardt-cloud-agent.fullname" -}}
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

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "reinhardt-cloud-agent.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels.
*/}}
{{- define "reinhardt-cloud-agent.labels" -}}
helm.sh/chart: {{ include "reinhardt-cloud-agent.chart" . }}
{{ include "reinhardt-cloud-agent.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels.
*/}}
{{- define "reinhardt-cloud-agent.selectorLabels" -}}
app.kubernetes.io/name: {{ include "reinhardt-cloud-agent.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use.
*/}}
{{- define "reinhardt-cloud-agent.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "reinhardt-cloud-agent.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Create the image reference. Falls back to .Chart.AppVersion when tag is empty.
*/}}
{{- define "reinhardt-cloud-agent.image" -}}
{{- if .Values.image.tag }}
{{- printf "%s:%s" .Values.image.repository .Values.image.tag }}
{{- else }}
{{- printf "%s:%s" .Values.image.repository .Chart.AppVersion }}
{{- end }}
{{- end }}

{{/*
Resolve the Secret name holding the control-plane auth token.
Prefers auth.existingSecret when set; otherwise falls back to the chart-created Secret.
*/}}
{{- define "reinhardt-cloud-agent.authSecretName" -}}
{{- if .Values.auth.existingSecret }}
{{- .Values.auth.existingSecret }}
{{- else }}
{{- printf "%s-auth" (include "reinhardt-cloud-agent.fullname" .) }}
{{- end }}
{{- end }}
