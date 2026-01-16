---
title: Creating a project
slug: /spacetimeauth/creating-a-project
---

:::warning

SpacetimeAuth is currently in beta, some features may not be available yet or may change in the future. You might encounter bugs or issues while using the service. Please report any problems you encounter to help us improve SpacetimeAuth.

:::

SpacetimeAuth is a service that can be enabled for any module published on
Maincloud. Check out our [Deploy to Maincloud](../../../00300-resources/00100-how-to/00100-deploy/00100-maincloud.md)
guide to learn how to publish a module.

## 1. Enabling SpacetimeAuth for a module

1. Deploy your module to Maincloud if you haven't already.
2. Navigate to the dashboard of your deployed module on Maincloud:
   1. Click on your profile picture in the top right corner.
   2. Select "My profile" from the dropdown menu.
   3. Click on the desired module from the list of your deployed modules.
      ![Module dashboard](/images/spacetimeauth/module-dashboard.png)
3. In the left sidebar, click on "SpacetimeAuth".

   ![Module sidebar](/images/spacetimeauth/module-sidebar.png)

4. Click on the "Use SpacetimeAuth" button.
   ![Enable SpacetimeAuth](/images/spacetimeauth/use-spacetimeauth.png)

## 2. Exploring the Dashboard

The dashboard provides you with multiple tabs to manage different aspects of
your project:

- **Overview**: A summary of your project, including a table of recent users.
- **Clients**: A list of all clients (applications) that can be used to
  authenticate in your applications.
  A default client is created for you when you create a new project.
- **Users**: A list of all users in your project, with options to search, filter,
  and manage users.
- **Identity Providers**: A list of all identity providers (e.g. Google, GitHub,
  etc.) that can be used to authenticate users in your project.
- **Customization**: Live editor to customize colors, logos, and authentication methods.

![Project overview](/images/spacetimeauth/project-overview.png)

## 4. Next Steps

Now that you have created a SpacetimeAuth project, you can start configuring it
to suit your application's needs. Check out our [configuration guide](/spacetimeauth/configuring-a-project)
for more information on setting up identity providers, customizing templates,
and managing users and roles.
