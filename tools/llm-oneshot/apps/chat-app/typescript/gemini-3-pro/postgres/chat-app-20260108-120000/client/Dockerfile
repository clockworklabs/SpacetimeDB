FROM node:20-alpine
# Refresh the OS CA bundle used by native TLS clients.
RUN apk add --no-cache --upgrade ca-certificates

WORKDIR /app

COPY package*.json ./
RUN npm install

COPY . .

EXPOSE 5173

CMD ["npm", "run", "dev", "--", "--host"]
