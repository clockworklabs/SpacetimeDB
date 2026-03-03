import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import helmet from 'helmet';
import dotenv from 'dotenv';
import { db } from './db';
import { setupSocketHandlers } from './socket';
import {
  cleanupExpiredMessages,
  setCleanupSocketServer,
} from './services/messageCleanup';
import {
  processScheduledMessages,
  setSocketServer,
} from './services/scheduledMessages';

// Load environment variables
dotenv.config();

const app = express();
const server = createServer(app);
const io = new Server(server, {
  cors: {
    origin: process.env.CLIENT_URL || 'http://localhost:3000',
    methods: ['GET', 'POST'],
  },
});

// Middleware
app.use(helmet());
app.use(cors());
app.use(express.json());

// Basic health check
app.get('/health', (req, res) => {
  res.json({ status: 'ok', timestamp: new Date().toISOString() });
});

// Socket.io setup
setupSocketHandlers(io);
setSocketServer(io);
setCleanupSocketServer(io);

// Start cleanup intervals
setInterval(cleanupExpiredMessages, 30000); // Clean expired messages every 30 seconds
setInterval(processScheduledMessages, 10000); // Process scheduled messages every 10 seconds

const PORT = process.env.PORT || 3001;

server.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});
