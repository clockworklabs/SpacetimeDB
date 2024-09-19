import { checkHeadingsOrder } from './check-headings.ts';
import { checkLinks } from './check-link.ts';
import { gatherData } from './gather-data.ts';

const data = await gatherData();

await checkHeadingsOrder(data);
console.log();
console.log();
console.log();
await checkLinks(data);
