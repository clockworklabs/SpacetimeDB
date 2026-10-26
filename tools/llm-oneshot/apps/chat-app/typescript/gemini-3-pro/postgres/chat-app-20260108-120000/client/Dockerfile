# Node 22.23.4 includes the eMudhra CA distrust fix.
FROM node:22.23.4-alpine

WORKDIR /app

COPY package*.json ./
RUN npm install

COPY . .

EXPOSE 5173

CMD ["npm", "run", "dev", "--", "--host"]
