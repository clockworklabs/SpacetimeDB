import { cronJobs, makeFunctionReference } from 'convex/server';

const crons = cronJobs();
crons.interval('deliver restored stock alerts', { seconds: 2 }, makeFunctionReference('progression:deliverRestoredAlerts'));
export default crons;
