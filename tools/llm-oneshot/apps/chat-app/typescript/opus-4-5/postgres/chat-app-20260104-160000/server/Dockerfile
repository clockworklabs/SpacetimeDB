# Node 22.23.4 includes the eMudhra CA distrust fix.
FROM node:22.23.4-alpine

WORKDIR /app

COPY package*.json ./
RUN npm install

COPY . .

# Wait for postgres and push schema
CMD ["sh", "-c", "sleep 3 && npm run db:push && npm run dev"]
