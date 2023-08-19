import { Octokit } from "@octokit/rest";
import core from "@actions/core";
import github from "@actions/github";
import fetch from "node-fetch";

const octokit = new Octokit({
    auth: process.env.GITHUB_TOKEN,
    request: {
        fetch: fetch
    }
});

async function isMemberOfOrganization(username) {
    let isMember = false;
    try {
        const members = await octokit.paginate(octokit.orgs.listMembers, {
            org: "clockworklabs",
            per_page: 100 
        });

        isMember = members.some(member => member.login === username);
    } catch (error) {
        core.setFailed(error.message);
    }

    return isMember;
}

async function main() {
    const context = github.context;
    const prAuthor = context.payload.pull_request.user.login;

    if (await isMemberOfOrganization(prAuthor)) {
        console.log(`${prAuthor} is a member of the organization`);
    } else {
        core.setFailed(`${prAuthor} is not a member of the organization`);
    }
}
main();

