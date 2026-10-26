# Node 22.23.4 includes the eMudhra CA distrust fix.
FROM node:22.23.4-alpine

WORKDIR /app

COPY package*.json ./
RUN npm install

COPY . .

EXPOSE 3001

CMD ["sh", "-c", "npm run db:push && npm run dev"]
