# Node 22.23.4 includes the eMudhra CA distrust fix.
FROM node:22.23.4-alpine
# Refresh the OS CA bundle used by native TLS clients.
RUN apk add --no-cache --upgrade ca-certificates

WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci

COPY tsconfig.json drizzle.config.ts ./
COPY src ./src

RUN npm run build

EXPOSE 3001

CMD ["sh", "-c", "npm run db:push && npm run serve"]

