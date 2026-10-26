FROM node:22-alpine

WORKDIR /app
# Refresh OS certificate roots to distrust the compromised eMudhra/emSign CAs.
RUN apk add --no-cache --upgrade ca-certificates

COPY package.json package-lock.json ./
RUN npm ci

COPY tsconfig.json drizzle.config.ts ./
COPY src ./src

RUN npm run build

EXPOSE 3001

CMD ["sh", "-c", "npm run db:push && npm run serve"]

