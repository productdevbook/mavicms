# syntax=docker/dockerfile:1

FROM oven/bun:1-alpine AS builder
WORKDIR /app

# Dependencies as their own layer: they only change when these two files do.
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile

COPY . .
RUN bun run build

FROM nginx:alpine AS runtime

# Under admin/ so that the paths the build wrote (/admin/assets/…) are the
# paths nginx finds on disk.
COPY --from=builder /app/dist /usr/share/nginx/html/admin
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
