# Node 22.23.4 includes the eMudhra CA distrust fix.
FROM node:22.23.4-alpine
# Refresh the OS CA bundle used by native TLS clients.
RUN apk add --no-cache --upgrade ca-certificates

WORKDIR /app

COPY package*.json ./
RUN npm install

COPY . .

EXPOSE 5174

CMD ["npm", "run", "dev", "--", "--host"]
