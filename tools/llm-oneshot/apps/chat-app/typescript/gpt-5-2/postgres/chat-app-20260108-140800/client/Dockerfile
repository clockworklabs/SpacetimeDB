FROM node:22-alpine

WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci

COPY tsconfig.json vite.config.ts index.html ./
COPY src ./src

EXPOSE 5173

CMD ["sh", "-c", "npm run dev -- --host 0.0.0.0 --port 5173"]

