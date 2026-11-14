{{/* The name everything of this installation begins with. */}}
{{- define "away.name" -}}
{{- if contains "away" .Release.Name -}}
{{- .Release.Name | trunc 50 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-away" .Release.Name | trunc 50 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{/* The labels that pick this installation's pods, and one part of them. */}}
{{- define "away.picks" -}}
app.kubernetes.io/name: away
app.kubernetes.io/instance: {{ .root.Release.Name }}
{{- with .part }}
app.kubernetes.io/component: {{ . }}
{{- end }}
{{- end -}}

{{/* The labels every object carries. */}}
{{- define "away.labels" -}}
{{ include "away.picks" . }}
app.kubernetes.io/version: {{ .root.Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .root.Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .root.Chart.Name .root.Chart.Version | replace "+" "_" }}
{{- end -}}

{{/* The image: by its digest where one is given, by its tag otherwise. */}}
{{- define "away.image" -}}
{{- if .Values.image.digest -}}
{{ .Values.image.repository }}@{{ .Values.image.digest }}
{{- else -}}
{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}
{{- end -}}
{{- end -}}

{{/*
One part: its account, which may ask the cluster for nothing, and its workload. Every
part runs the one image as an ordinary user, with files it can't write and no
capabilities, so it fits the strictest profile a cluster has.
  root   the chart's context
  part   server, gateway or issuer
  port   where it answers what it is there for, if it has such an address
  named  that port's name
*/}}
{{- define "away.part" -}}
{{- $name := printf "%s-%s" (include "away.name" .root) .part -}}
{{- $values := index .root.Values .part -}}
apiVersion: v1
kind: ServiceAccount
metadata:
  name: {{ $name }}
  labels:
    {{- include "away.labels" . | nindent 4 }}
automountServiceAccountToken: false
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ $name }}
  labels:
    {{- include "away.labels" . | nindent 4 }}
spec:
  replicas: 1
  selector:
    matchLabels:
      {{- include "away.picks" . | nindent 6 }}
  template:
    metadata:
      labels:
        {{- include "away.labels" . | nindent 8 }}
    spec:
      serviceAccountName: {{ $name }}
      automountServiceAccountToken: false
      {{- with .root.Values.image.pullSecrets }}
      imagePullSecrets:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      securityContext:
        runAsNonRoot: true
        seccompProfile:
          type: RuntimeDefault
      containers:
        - name: {{ .part }}
          image: {{ include "away.image" .root | quote }}
          imagePullPolicy: {{ .root.Values.image.pullPolicy }}
          args: [{{ .part | quote }}]
          ports:
            {{- if .port }}
            - name: {{ .named }}
              containerPort: {{ .port }}
            {{- end }}
            - name: health
              containerPort: 9090
          livenessProbe:
            httpGet:
              path: /livez
              port: health
          readinessProbe:
            httpGet:
              path: /readyz
              port: health
            periodSeconds: 5
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities:
              drop: [ALL]
          {{- with $values.resources }}
          resources:
            {{- toYaml . | nindent 12 }}
          {{- end }}
      {{- with .root.Values.nodeSelector }}
      nodeSelector:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .root.Values.tolerations }}
      tolerations:
        {{- toYaml . | nindent 8 }}
      {{- end }}
{{- end -}}
