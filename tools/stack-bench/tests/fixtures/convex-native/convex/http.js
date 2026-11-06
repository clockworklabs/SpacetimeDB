import { httpRouter } from 'convex/server';
import { auth } from './auth.js';
const http = httpRouter();
auth.addHttpRoutes(http);
export default http;
