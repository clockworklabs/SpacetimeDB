# Notes for maintainers

The directory `src/client_api` is generated from [the SpacetimeDB client-api-messages](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/client-api-messages).
This is not automated.
Whenever the `client-api-messages` crate changes, you'll have to manually re-generate the definitions.
See that crate's DEVELOP.md for how to do this.

The generated files must be manually modified to fix their imports from the rest of the SDK.
Within each generated file:

- Change the import from `"@clockworklabs/spacetimedb-sdk"` to `"../index"`.
- If the type has generated a `class`, remove its `extends DatabaseTable`, remove the `public static db` member, and remove the call to `super()` within the constructor.

## Releases and publishing

Every Pull Request with public-facing change(Bug fix, perf, feature etc) must be accompanied by a changeset. Any person working on a patch or feature needs to run `pnpm -w changeset` command, which will prompt them to select packages changed. Choose `@clockworklabs/spacetimedb-sdk`

![image](https://github.com/user-attachments/assets/3a69ff1f-c92b-459a-8dcc-d8fea53f77b4)

Next it will ask whether you'd like to add a Major tag to it. Hit enter to go to minor tag. If its a minor change(In our case, minor is major until v1 comes out, as in every minor can have breaking changes). If its a patch change(Or minor for prerelease time), then again hit enter

After selecting the correct tag, it will ask you for a message

![image](https://github.com/user-attachments/assets/d05a338b-965d-4669-8155-542d0225b257)
![image](https://github.com/user-attachments/assets/7abc830e-4590-42e7-bce8-86155d86c672)
![image](https://github.com/user-attachments/assets/8f3b16bd-b01d-4117-8d02-3887f1d308dd)

Once that is done, hit enter. It will generate a `.md` file which you can then push to github. This all has to be done in the PR with the feature/fix in it.

We can merge it instantly to do a release, or we can merge PRs with their own Changesets. For eg, any new feature or patch we work on for 0.12 now, should have a Changeset in it. All of these will accumulate in the "Version Packages" PR. Once all these are satisfactorily done, we merge this PR, which will

- Release the package on npm
- Release on Github tags
- Update CHANGELOG.md

**NOTE: It is very important that no one manually runs `npm publish`. We have provenance enabled on this package, means each version will be signed by github and traceable to the very commit associated to it**

Publishing manually will breach the provenance contract, and alert security servcies like Snyk into investigating the package or issuing a warning. npm install of our package will also warn them of potential compromise to the package

![image](https://github.com/user-attachments/assets/b56282b7-9055-48a0-8a49-3df9d75d481f)
![image](https://github.com/user-attachments/assets/99d023cf-31cc-48a0-93ed-a88c326425c5)
