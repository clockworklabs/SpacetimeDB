# **Blackholio**

**Blackholio** is a small-scoped MMORPG built using Unity and [SpacetimeDB](https://spacetimedb.com), designed to showcase scalable multiplayer game development. Inspired by [agar.io](https://agar.io), **Blackholio** reimagines the mechanics with a space theme where players become planets, stars, and black holes in a competitive cosmic arena.

### **Game Overview**
- **Gameplay:** Absorb smaller entities, grow, evolve, and dominate the leaderboard as a black hole.
- **Scale:** Supports hundreds of players seamlessly with SpacetimeDB's real-time synchronization.
- **Objective:** Demonstrate SpacetimeDB's features and best practices in a fun, interactive project.

---

## **Tutorial Overview**

This repository accompanies the **Blackholio Unity Tutorial**, which guides you through building the game from scratch while learning:
- **Unity integration** with SpacetimeDB.
- Client-server setup for multiplayer games.
- SpacetimeDB features, including reducers, tables, and scheduled events.

By following the tutorial, you'll develop:
1. A basic understanding of SpacetimeDB for multiplayer games.
2. A functional game prototype with scalable multiplayer features.

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

---

### **Getting Started**
To get started, follow these steps:

1. Clone this repository:
   ```bash
   git clone https://github.com/ClockworkLabs/Blackholio.git
   cd Blackholio
   ```
2. Install [Unity Hub](https://unity.com/download) and Unity **2022.3.32f1 LTS**.
3. Follow the tutorial parts:
   - [Part 1: Setup](https://github.com/ClockworkLabs/Blackholio/tree/master/docs/unity/part-1)
   - [Part 2: Connecting to SpacetimeDB](https://github.com/ClockworkLabs/Blackholio/tree/master/docs/unity/part-2)
   - [Part 3: Gameplay](https://github.com/ClockworkLabs/Blackholio/tree/master/docs/unity/part-3)
   - [Part 4: Moving and Colliding](https://github.com/ClockworkLabs/Blackholio/tree/master/docs/unity/part-4)

4. Run the game using the Unity Editor or build for your platform.

---

### **Features**
- **Core Gameplay**: Consume, grow, and dominate.
- **Multiplayer**: Scales to hundreds of players with SpacetimeDB.
- **Dynamic Gameplay**: Spawning, movement, collisions, and evolution mechanics.
- **Learn by Building**: Explore the mechanics of Unity and SpacetimeDB through the tutorial.

---

### **Repository Structure**

```plaintext
Blackholio/
├── client-unity/            # Unity client project
├── server-rust/       # SpacetimeDB server module (Rust implementation)
├── server-csharp/     # SpacetimeDB server module (C# implementation)
├── docs/              # Tutorial documentation and images
└── README.md          # This file
```

---

### **Requirements**
- **Unity**: Version `2022.3.32f1 LTS` or later.
- **Rust**: Version `1.65.0` or later (for the SpacetimeDB server module).
- **SpacetimeDB CLI**: Installed via [SpacetimeDB installation guide](https://spacetimedb.com/docs/install).

---

### **Resources**
- [SpacetimeDB Documentation](https://spacetimedb.com/docs/)
- [Join our Discord Community](https://discord.gg/spacetimedb)
- [Agar.io (inspiration)](https://agar.io)

---

### **License**
This project is licensed under the [Apache License](LICENSE).

Feel free to fork, modify, and use **Blackholio** as a starting point for your own projects. Contributions are welcome!

---

### **Feedback**
We'd love to hear your thoughts on the tutorial or game! Open an issue in the repository or chat with us on [Discord](https://discord.gg/spacetimedb).  

Happy developing, and enjoy creating the cosmos with **Blackholio**! 🚀