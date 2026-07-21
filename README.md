# Bad Apple 🍎 in Rust Terminal

Ever wanted to see the legendary "Bad Apple!!" video rendered entirely in ASCII art right in your terminal? This minimal Rust CLI tool does exactly that. It dynamically scales to fit your terminal size and perfectly centers the frames—all while keeping dependencies to a bare minimum. 

## 🚀 Quick Start

1. **Clone the repo**
   ```bash
   git clone https://github.com/adi-IL/bad_apple_rs.git
   cd bad_apple_rs
   ```

2. **Build the frames**
   Make sure you have your extracted frame PNGs in a folder named `frames`.
   ```bash
   cargo run --release -- build
   ```

3. **Play the animation!**
   ```bash
   cargo run --release -- play
   ```

## 🛠️ Requirements

- Rust and Cargo
- A terminal of your choice

## 📝 License

This project is licensed under the MIT License.
