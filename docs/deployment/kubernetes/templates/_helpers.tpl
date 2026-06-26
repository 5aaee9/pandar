{{/*
Expand the name of the chart.
*/}}
{{- define "pandar.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "pandar.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "pandar.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "pandar.labels" -}}
helm.sh/chart: {{ include "pandar.chart" . }}
app.kubernetes.io/name: {{ include "pandar.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{- define "pandar.selectorLabels" -}}
app.kubernetes.io/name: {{ include "pandar.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "pandar.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "pandar.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}

{{- define "pandar.hubDatabaseSecretName" -}}
{{- if .Values.hub.database.existingSecret -}}
{{- .Values.hub.database.existingSecret -}}
{{- else -}}
{{- printf "%s-hub-database" (include "pandar.fullname" .) -}}
{{- end -}}
{{- end -}}
