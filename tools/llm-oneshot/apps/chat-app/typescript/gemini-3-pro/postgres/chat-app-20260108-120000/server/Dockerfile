FROM node:20-alpine

WORKDIR /app

COPY package*.json ./
RUN npm install

COPY . .

EXPOSE 3001

CMD ["sh", "-c", "npm run db:push && npm run dev"]
