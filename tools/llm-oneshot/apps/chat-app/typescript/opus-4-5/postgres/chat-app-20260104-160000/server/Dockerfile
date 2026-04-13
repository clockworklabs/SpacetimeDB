FROM node:20-alpine

WORKDIR /app

COPY package*.json ./
RUN npm install

COPY . .

# Wait for postgres and push schema
CMD ["sh", "-c", "sleep 3 && npm run db:push && npm run dev"]
