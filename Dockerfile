# ============================================================================
# app-template — public entry-point der loader host shell via Native Federation
# Ingen @bimo-dk dependencies — simpel Angular build.
# ============================================================================

FROM node:22-alpine AS builder
WORKDIR /app

COPY package*.json ./
RUN npm install --no-audit --no-fund --legacy-peer-deps

ARG HOST_REMOTE_ENTRY=/host/remoteEntry.json
ARG NEXUS_TOKEN=dev-token-change-in-production

COPY tsconfig*.json angular.json federation.config.js ./
COPY src ./src
COPY public ./public

RUN node -e "const fs=require('fs'); \
let envPath='src/environments/environment.prod.ts'; \
let envFile=fs.readFileSync(envPath,'utf8'); \
envFile=envFile.replace('HOST_REMOTE_ENTRY_PLACEHOLDER', process.env.HOST_REMOTE_ENTRY || '/host/remoteEntry.json'); \
fs.writeFileSync(envPath, envFile); \
let intPath='src/app/interceptors/nexus-auth.interceptor.ts'; \
let intFile=fs.readFileSync(intPath,'utf8'); \
intFile=intFile.replace('NEXUS_TOKEN_PLACEHOLDER', process.env.NEXUS_TOKEN || 'dev-token'); \
fs.writeFileSync(intPath, intFile);" \
  HOST_REMOTE_ENTRY=${HOST_REMOTE_ENTRY} NEXUS_TOKEN=${NEXUS_TOKEN}

RUN npm run build:prod

FROM nginx:alpine
RUN apk add --no-cache wget gettext

COPY --from=builder /app/dist/app/browser /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY docker-entrypoint.d/40-runtime-config.sh /docker-entrypoint.d/40-runtime-config.sh
RUN chmod +x /docker-entrypoint.d/40-runtime-config.sh

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=10s --start-period=15s --retries=3 \
  CMD wget -qO- http://localhost/health || exit 1

CMD ["nginx", "-g", "daemon off;"]
