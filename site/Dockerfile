# The clove site: built from the repo (the docs pages read README.md and
# docs/), then served as static files by Caddy.
# Railway: set RAILWAY_DOCKERFILE_PATH=site/Dockerfile, root directory unset.
FROM node:22-alpine AS build
RUN corepack enable
WORKDIR /repo
COPY README.md ./
COPY docs ./docs
COPY site/package.json site/pnpm-lock.yaml ./site/
WORKDIR /repo/site
RUN pnpm install --frozen-lockfile
COPY site ./
RUN pnpm build

FROM caddy:2-alpine
COPY site/Caddyfile /etc/caddy/Caddyfile
COPY --from=build /repo/site/dist /srv
