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

async function main() {
    let prAuthor;

    try {
        const context = github.context;
        const owner = context.repo.owner;
        const repo = context.repo.repo;
        prAuthor = context.payload.pull_request.user.login;

        // Check if the PR author is a member of the organization
        const isMember = await octokit.orgs.checkMembershipForUser({
            org: owner,
            username: prAuthor,
        });

        if (!isMember) {
            core.setFailed(`${prAuthor} is not a member of the organization`);
        } else {
            console.log(`${prAuthor} is a member of the organization`);
        }
    } catch (error) {
        if (error.status === 404) {
            core.setFailed(`${prAuthor} is not a member of the organization`);
        } else {
            core.setFailed(error.message);
        }
    }
}

main();

