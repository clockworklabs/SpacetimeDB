FROM node:20-alpine

WORKDIR /app
# Refresh OS certificate roots to distrust the compromised eMudhra/emSign CAs.
RUN apk add --no-cache --upgrade ca-certificates

COPY package*.json ./
RUN npm install

COPY . .

EXPOSE 3001

CMD ["sh", "-c", "npm run db:push && npm run dev"]
