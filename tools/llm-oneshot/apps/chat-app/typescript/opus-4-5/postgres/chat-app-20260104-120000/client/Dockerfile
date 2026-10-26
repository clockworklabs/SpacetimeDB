# Node 22.23.4 includes the eMudhra CA distrust fix.
FROM node:22.23.4-alpine AS builder
# Refresh the OS CA bundle used by native TLS clients.
RUN apk add --no-cache --upgrade ca-certificates

WORKDIR /app

COPY package*.json ./
RUN npm install

COPY . .
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
