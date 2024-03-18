<h3 align="center">
	<img src="https://bashlogo.com/img/symbol/svg/full_colored_light.svg" width="16" alt="Logo"/>
	arg.sh
</h3>

<h6 align="center">
  <a href="https://arg.sh/getting-started">Quickstart</a>
  ·
  <a href="https://arg.sh/command-line-parser">CLI Parser</a>
  ·
  <a href="https://arg.sh/libraries/overview">Libraries</a>
  ·
  <a href="https://arg.sh/styleguide">Styleguide</a>
</h6>

<p align="center">
	<a href="https://github.com/arg-sh/argsh/stargazers">
		<img alt="Stargazers" src="https://img.shields.io/github/stars/arg-sh/argsh?style=for-the-badge&logo=starship&color=C9CBFF&logoColor=D9E0EE&labelColor=302D41"></a>
	<a href="https://github.com/arg-sh/argsh/releases/latest">
		<img alt="Releases" src="https://img.shields.io/github/release/arg-sh/argsh.svg?style=for-the-badge&logo=github&color=F2CDCD&logoColor=D9E0EE&labelColor=302D41"/></a>
	<a href="https://github.com/arg-sh/argsh/issues">
		<img alt="Issues" src="https://img.shields.io/github/issues/arg-sh/argsh?style=for-the-badge&logo=gitbook&color=B5E8E0&logoColor=D9E0EE&labelColor=302D41"></a>
	<a href="https://discord.gg/VsQpUQX3Zr">
		<img alt="Discord" src="https://img.shields.io/discord/1216780297727770746?style=for-the-badge&logo=discord&color=DDB6F2&logoColor=D9E0EE&labelColor=302D41"></a>
</p>

&nbsp;

<p align="left">
Bash is a powerful tool (and widly available), but it's also a language that is easy to write in a way that is hard to read and maintain. As such Bash is used often but used as little as possible, resulting in poor quality scripts that are hard to maintain and understand.

Not only is this happaning as Bash is seen as a "glue" language, but also because there is no hardend styleguide, easy testing and good documentation around it.

The Google Shell Style Guide says it itself:

> If you are writing a script that is more than 100 lines long, or that uses non-straightforward control flow logic, you should rewrite it in a more structured language now.

You can write bad code in every other language too, but there is lots of effort to make it better. So let's make it better for bash too. Let's make Bash a more structured language.

This is what argsh is trying to do. Check out the [Quickstart](https://arg.sh/getting-started) to see how you can use it.
</p>

&nbsp;

### 🧠 Design Philosophy

- **First class citizen**: Treat your scripts as first class citizens. They are important and should be treated as such.
- **Be Consistent**: Consistency is key. It makes your scripts easier to read and maintain.
- **Perfect is the enemy of good**: Don't try to make your scripts perfect. Make them good and maintainable.
- **Write for the next person**: Write your scripts for the next person that has to read and maintain them. This person might be you.

&nbsp;

### 🚧 State of this Project

> This project is in a very early stage. It's not even alpha. It's more like a concept. It's not even a concept, it's more like a thought. It's not even a thought, it's more like a dream. It's not even a dream, it's more like a wish.
> 
> Quote by Copilot

That beeing said, most of it is quite rough. But it's a start. The best time that you join the conversation and try to refine the concept.

#### Short term goals

- [ ] Make `.bin/argsh` more generic for other projects to use
- [ ] Clean up the `www/` folder from unused medusajs files
- [ ] Design a logo
- [ ] Provide a set of code snippets
- [ ] A complete styleguide
- [ ] Best practices (like error handling, logging, json, etc.)
- [ ] Generate and easy integration of tests
- [ ] Generate documentation
- [ ] Easy use of bash debugger
- [ ] Write a language server to lint and format bash code acording to the styleguide
- [ ] VSCode extension for the language server
- [ ] Easy bootstrap, minimal dependencies, easy to implement
- [ ] Convert [shdoc](https://github.com/reconquest/shdoc) to rust
- [ ] Convert [obfus](./bin/obfus) to rust or rewrite it in rust/shfmt, at least make it more robust (remove sed)

&nbsp;

### 📜 License

Argsh is released under the MIT license, which grants the following permissions:

- Commercial use
- Distribution
- Modification
- Private use

For more convoluted language, see the [LICENSE](https://github.com/arg-sh/argsh/blob/main/LICENSE). Let's build a better Bash experience together.

&nbsp;

### ❤️ Gratitude

Thanks to the following tools and projects developing this project is possible:

- [medusajs](https://github.com/medusajs/medusa/): From where the base of this docs, github and more is copied.
- [Google Styleguide](https://google.github.io/styleguide/shellguide.html): Google's Shell Style Guide used as base for the argsh styleguide.
- [Catppuccin](https://github.com/catppuccin/catppuccin): Base for the readme.md and general nice color palettes.

&nbsp;

### 🐾 Projects to follow

- [bash-it](https://github.com/Bash-it/bash-it): A Bash shell - autocompletion, themes, aliases, custom functions, and more.

&nbsp;

<p align="center">Copyright &copy; 2024-present <a href="https://github.com/fentas" target="_blank">Jan Guth</a>
<p align="center"><a href="https://github.com/arg-sh/argsh/blob/main/LICENSE"><img src="https://img.shields.io/static/v1.svg?style=for-the-badge&label=License&message=MIT&logoColor=d9e0ee&colorA=302d41&colorB=b7bdf8"/></a></p>