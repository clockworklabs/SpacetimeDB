# Build from the repo root:
#   docker build -f tools/llm-benchmark-site/Dockerfile -t llm-benchmark-site .
#   docker run -p 8080:80 llm-benchmark-site

# ---------- stage 1: build ----------
FROM node:20-alpine AS build
WORKDIR /site

COPY tools/llm-benchmark-site/package.json tools/llm-benchmark-site/package-lock.json* ./
RUN npm ci

COPY tools/llm-benchmark-site/ .
RUN npm run build

# ---------- stage 2: serve ----------
FROM nginx:alpine

# App bundle
COPY --from=build /site/dist /usr/share/nginx/html

# JSON data files served at /docs/llms/ so the app's relative fetch paths resolve correctly
COPY docs/llms/ /usr/share/nginx/html/docs/llms/

# SPA routing: unknown paths fall through to index.html
COPY tools/llm-benchmark-site/nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
