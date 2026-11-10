# Node 22.23.4 includes the eMudhra CA distrust fix.
FROM node:22.23.4-alpine AS builder

WORKDIR /app
# Refresh OS certificate roots to distrust the compromised eMudhra/emSign CAs.
RUN apk add --no-cache --upgrade ca-certificates

COPY package*.json ./
RUN npm install

COPY . .
RUN npm run build

FROM nginx:alpine

COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"]
