# Rotproof

[English](https://github.com/secta113/rotproof/blob/main/README.md) | 日本語

<!-- Translated from README.md at d04f9361. `cargo xtask ci` fails when README.md is another text: bring this file to
it, then write the hash the failure names here. -->

Rotproof は、LLM と人がコードを変えていくあいだにプロジェクトの構造がずれて腐っていくのを防ぎます。言語のランタイムを
必要としない 1 つのバイナリで、プロジェクトの CI が実行するコマンドは `rotproof check` です。コードがプロジェクトの
宣言した層を破ったとき、またはコードが変わったのにそれを説明する文書が置き去りになったとき、このコマンドが失敗します。

この文書は [README.md](https://github.com/secta113/rotproof/blob/main/README.md) の日本語訳です。開発とリリースの
節は訳していません。英語版と食い違うときは、英語版が正です。仕様
（[SPECIFICATION.md](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md)）は英語だけです。

## 何が嬉しいか

リポジトリのコードを LLM のエージェントが書くとき、次のことをレビューで人が見つけなくて済みます。

- **エージェントが層をまたぐと、ビルドが落ちます。** すべての import を層の表に照らします。表はオニオンアーキテクチャで、
  `composition`、`infrastructure`、`application`、`protocol`、`domain` と、画面のための `ui`、`presentation` です。
  Python、TypeScript、Rust を検査します。この層に分けられないコードは責務が混ざっているので、表はエージェントに
  コードを分けさせる手段でもあります。例外は、人が端末で承認したエントリだけで、レビューする人は差分でそれを見ます。
- **コードからは読み取れないことが、正しいまま残ります。** コードの背後にある理由は knowledge の文書に書き、文書は
  自分が説明するコードを名指して、ハッシュで固定します。そのコードが変わると、誰かが文書をコードに照らして読み、
  固定し直すまで `rotproof check` が失敗します。
- **先送りした作業が、コメントに隠れません。** コードのコメントの `TODO`、`FIXME`、`XXX`、`HACK`、`NOTE` は検査を
  失敗させます。見つけたことは、いま直すか、Issue に書くか、手放すかのどれかです。型検査を止めるコメントも通りません。
  環境変数を読めるのは `infrastructure` だけです。
- **バイナリしか持たないエージェントも、迷いません。** `rotproof --help` が始める順番を示します。規則はプロジェクトの
  スタックに合わせて `.rotproof/AGENTS.md` に書かれ、`rotproof create` が書く `AGENTS.md` と `CLAUDE.md` が
  エージェントをそこへ案内します。
- **規則は、固定した版のものです。** プロジェクトは Rotproof の版を 1 つ、厳密に固定します。規則が変わるのは、
  プロジェクト自身のコミットの中だけです（「互換性」を参照）。

Rotproof がやらないこと: `rotproof check` は報告するだけで、何も直しません。文書がいまも正しいかどうかは判断できず、
分かるのは、その下のコードが変わったことだけです。読み直すのは、人か、変更を加えたエージェントです。

## インストール

Rotproof は Windows x86_64、Linux x86_64（glibc 2.17 以上）、macOS arm64（Apple silicon、macOS 11 以上）向けに
出しています。版は厳密に固定してください。アップグレードは新しい規則を持ち込みうるので、それだけのコミットにするためです。

```sh
pip install rotproof==1.0.0
```

Node のプロジェクトでは npm で固定します。プラットフォームに合ったバイナリも一緒に入ります（パッケージ `rotproof` と、
`@rotproof/linux-x64`、`@rotproof/win32-x64`、`@rotproof/darwin-arm64` のいずれか）。

```sh
npm install -D -E rotproof@1.0.0
```

wheel に入っているのはバイナリだけで、Python のコードは動きません。Python も Node も無い場合は、
[GitHub Releases](https://github.com/secta113/rotproof/releases) からプラットフォームに合ったアーカイブを取り、
そこにある `SHA256SUMS` と照らしてください。アーカイブにはバイナリと `LICENSE-MIT`、`LICENSE-APACHE`、
`THIRD-PARTY-LICENSES.txt` が入っています。1.0.0-rc.2 の次のリリースから、リリースは変更できません。公開された
アーカイブは差し替えられないので、1 つの版は 1 つのバイナリを指します。`rotproof create` が作る `rust` と `none` の
プロジェクトは、CI でこの方法で Rotproof を入れます。

ほかのプラットフォームでは、pip はインストールできる Rotproof を見つけられません。[rustup](https://rustup.rs/) を
入れて、ソースからビルドしてください（ツールチェーンの版は `rust-toolchain.toml` から決まります）。

```sh
cargo build --release   # バイナリは target/release/rotproof（Windows では rotproof.exe）
```

## チュートリアル: 5 分

新しい Python のプロジェクトを作り、わざと層を破り、knowledge の文書にコードを追わせます。`PATH` に `rotproof` が
あること（有効にした仮想環境で `pip install` すれば入ります）と、テキストエディタが要ります。プロジェクトは Git の
リポジトリでなくてもかまいません。Python は実行しません。出力は、これを書いたときに Rotproof が実際に出したもので、
省いた行は `...` にしてあります。

### 1. プロジェクトを作る

```sh
mkdir shop
cd shop
rotproof init --stack python --ui none
```

```text
wrote .config/rotproof.toml
next: declare in absent the layers this project does not have, then run `rotproof create`, which lists the layers it would make, and `rotproof create --yes` to make them
```

`init` が書くのは宣言 `.config/rotproof.toml` だけです。これが、このプロジェクトが宣言する構造です。スタックと、
画面の無いプロジェクト（コマンドラインの道具、ライブラリ、サーバー）のための `--ui none` が入っています。`absent` には
このプロジェクトが持たない層を並べますが、今回は 5 つとも持つので、そのままにします。`rotproof create` だけを
実行すると、作る層の一覧を出すだけで何も書きません。先に一覧を読めるようにするためです。`--yes` を付けると作ります。

```sh
rotproof create --yes
```

```text
wrote composition/__init__.py
...
wrote domain/__init__.py
wrote docs/log.md
wrote .rotproof/AGENTS.md
...
wrote docs/knowledge/index.md
next: run `rotproof check`; the rules it keeps are in .rotproof/AGENTS.md
```

層はそれぞれディレクトリで、`__init__.py` にその役割が書いてあります。`docs/` には記録が入ります。残りは、エージェントが
読むもの（`AGENTS.md` と `CLAUDE.md`。どちらも、スタックに合わせて書かれた規則 `.rotproof/AGENTS.md` を指します）と、
`rotproof check` を実行する CI のワークフローです。新しいプロジェクトは、何も書かなくても最初の検査に通ります。

```sh
rotproof check
```

```text
the layers and every record keep the rules
```

### 2. わざと層を破る

`domain` はいちばん内側の層で、ほかの層を import しません。import させてみます。`infrastructure/outbound/db.py` を
作ります。

```python
def load_rate(currency: str) -> float:
    return 1.0
```

それを使う `domain/price.py` を作ります。

```python
from infrastructure.outbound.db import load_rate


def convert(amount: float, currency: str) -> float:
    return amount * load_rate(currency)
```

```sh
rotproof check
```

```text
the layers import only what the layer table in .rotproof/AGENTS.md allows:
  domain/price.py:1: imports infrastructure.outbound.db.load_rate, in infrastructure.outbound; domain imports no other layer
```

終了コードは 1 で、ファイルと行と規則を名指します。ここで名前の出る表は Rotproof に組み込まれていて、
`.rotproof/AGENTS.md` に載っています。層が求めるとおりに直します。`domain` はレートを引数で受け取り、外側の層の
呼び出す側がレートを読みます。`domain/price.py` を置き換えます。

```python
def convert(amount: float, rate: float) -> float:
    return amount * rate
```

`rotproof check` は、ふたたび `the layers and every record keep the rules` と出します。

### 3. 文書にコードを追わせる

`convert` がレートを受け取る理由は、コードには書いてありません。そういう知識は `docs/knowledge/` に、説明するコードを
名指す文書として置きます。文書は 1 つの領域（area）に属するので、先に領域を宣言します。`.config/rotproof.toml` の
`areas = []` を次のように変えます。

```toml
areas = ["pricing"]
```

`docs/knowledge/rates.md` を作ります。

```markdown
---
type: Knowledge
title: A price is converted with the rate it is handed
description: 'convert does not look the rate up: the rate comes from the bank feed, which domain may not reach, so a caller passes it in.'
tags: [pricing]
status: stable
---

The rate is read from the bank's feed in `infrastructure`, and `domain` imports no other layer, so `convert` takes it as
an argument. Looking it up inside `convert` was tried first and was rejected by `rotproof check`.
```

`docs/knowledge/` の索引は生成するもので、手では書きません。書き直すまで `rotproof check` は失敗します。

```sh
rotproof check
```

```text
every generated file is up to date:
  out of date, run `rotproof index`: docs/knowledge/index.md
```

`rotproof index` を実行すると、索引が書かれます。次に、文書に `convert` を追わせます。文書をコードに照らして読み、
固定します。

```sh
rotproof follows knowledge/rates.md domain/price.py::convert
```

```text
  domain/price.py::convert: added 08e2917d
wrote docs/knowledge/rates.md
```

文書の frontmatter に `follows` が入り、`convert` のソースのハッシュが書かれました。

```yaml
follows:
  domain/price.py::convert: "08e2917d"
```

`rotproof check` は通ります。ここでコードを変えて、結果をセント単位に丸めます。`domain/price.py` です。

```python
def convert(amount: float, rate: float) -> float:
    return round(amount * rate, 2)
```

```sh
rotproof check
```

```text
every knowledge document matches the code it follows:
  knowledge/rates.md follows domain/price.py::convert, which changed since the document was last reviewed: review the document against it, then run `rotproof follows knowledge/rates.md domain/price.py::convert`
```

また終了コード 1 です。文書はもう間違っているかもしれず、ほかには誰もそれを知らせてくれません。Rotproof は文書の
正しさを判断しません。ハッシュが言うのは、コードが変わったことだけです。文書を新しいコードに照らして読み、足りない
ことを足します（`docs/knowledge/rates.md` の末尾に 1 行、`The result is rounded to cents.`）。それから、メッセージが
名指したコマンドで固定し直します。

```sh
rotproof follows knowledge/rates.md domain/price.py::convert
```

```text
  domain/price.py::convert: 08e2917d -> d2e4eb3b
wrote docs/knowledge/rates.md
```

`rotproof check` は通ります。コードを変えたコミットには、読み直した文書が一緒に入ります。

### ここから先

プロジェクトをコミットしてください。作られた CI のワークフローは push のたびに `rotproof check` を実行し、
`requirements-dev.txt` は Rotproof を厳密な版に固定しています。`rotproof check` が検査することの詳細、コマンド、
スタックごとの層は[仕様](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md)（英語）にあります。
Rotproof のアップグレードで何が変わりうるかは「互換性」に書いてあります。

## 互換性

1.0.0 から、リリースは版の 1 か所だけを上げます。どこを上げるかは、前のリリースからの変更のうち、いちばん壊すものが
決めます。

- **メジャー**（`1.2.0` から `2.0.0`）: 前の版でプロジェクトがしていたことが、この版では失敗するか、別の動きをしうる。
  `rotproof check` の新しい規則、または厳しくなった規則。コマンド、フラグ、`.config/rotproof.toml` の欄が消えた、
  名前が変わった、意味が変わった。成功していたコマンドラインが止まる、または別の終了コードで終わる。
  `rotproof check` が誤って通していたものを落とす修正。
- **マイナー**（`1.1.3` から `1.2.0`）: 動いていたものはすべて同じように動き、新しく使えるものがある。新しいコマンド、
  新しい任意のフラグ、宣言の新しい任意の欄。
- **パッチ**（`1.1.2` から `1.1.3`）: 動いていたものはすべて同じように動き、新しいものは無い。Rotproof が誤って
  落としていたものを通す修正、クラッシュを止める修正、メッセージの文言、ドキュメント。

したがって、1.0.0 で動いていたものを失敗させたり別の動きにしたりする変更は、2.0.0 まで待ちます。新しい規則が
マイナーやパッチで入ることはありません。迷ったときはメジャーにします。1 つだけ、壊す変更の印ではないものが
あります。アップグレードはすべて、パッチも含めて、`rotproof init` を実行するまで `rotproof check` が失敗します。
宣言の `files` が、プロジェクトのファイルがどの版に追いついているかを示しているからです
（[Upgrading Rotproof](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#upgrading-rotproof) を参照）。

メジャーは先にリリース候補（`2.0.0-rc.1`）として出し、実際の作業で使ってからリリースします。候補と候補のあいだでは
何が壊れてもおかしくないので、候補を固定するのは試すときだけにしてください。マイナーとパッチは直接出します。pip が
プレリリースを入れるのは、その版を固定したときか `--pre` を付けたときだけです。npm では dist-tag `next` で公開します。

プロジェクトは厳密な版を固定するので、アップグレードはいつもプロジェクト自身のコミットです。版は、そのコミットを作る
人にどれだけ読めばよいかを伝えます。パッチは見ずに取り込め、マイナーは新しく使えるものがあり、メジャーはリリース
ノートを読む必要があります。1.0 より前は、マイナーの版がメジャーの役割をしていました。

## Rotproof が保つもの

保つ構造は 2 つです。

- **層:** コードのどの部分がどの部分を import してよいか。オニオンアーキテクチャに基づきます（`composition`、
  `infrastructure`、`application`、`protocol`、`domain` と、画面のための `ui`、`presentation`）。この層に
  分けられないコードは責務が混ざっているので、層は LLM や人にコードを分けさせる手段です。どのコードも、役割と
  許される import が書かれた層のどれかに収まらなければなりません。Rotproof はプロジェクトの開始時に層を作り、
  実行のたびに検査するので、依存の向きは意図したとおりに保たれます。
- **記録:** いまどうなっているか、なぜそうなのかのうち、コードからは読み取れないこと。これは、説明するコードを追う
  knowledge の文書に書き、その下のコードが変わると `rotproof check` が失敗します（目にしたときだけ意味を持つ
  ことのための alarm もあります）。もう 1 つは、各タスクが残したものの log です。未完了の作業は、ここではなく
  Issue に置きます。知識も、見つけたことも、記録の外には残しません。コードのコメントの `TODO` や `NOTE` は検査を
  失敗させます。

## 原則

- **宣言が正であり、Rotproof は直さない。** プロジェクトが宣言した構造が、その構造です。それと違う木は、どちら向きの
  違いでも失敗し、木と宣言のどちらが間違っているかは誰かが決めます。`rotproof check` は報告するだけです。書くのは、
  人やエージェントが実行するコマンドで、何をしたかを言います。そういうコマンドは、足りないものを足し、人が書いた値は
  そのままにします。足すのは、無い層やプロジェクトのファイル、宣言に無い欄、人が承認したエントリ、`rotproof init` の
  名前の付いた更新が持ち込むもの
  （[Upgrading Rotproof](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#upgrading-rotproof) を参照）、
  そして誰も手で書かない、Rotproof が生成するファイルです。宣言に無い欄は、最初の値を一度だけ木から取ります
  （`areas` には knowledge の文書が使っているタグが入るので、規則を守っていた文書は守ったままになります）。それ以降、
  宣言に無いタグを持つ文書は失敗します。`.claude/settings.json` の更新は、キーとその順番と値を保ちますが
  （取り除くのは、Rotproof が書いた stop hook と time hook に厳密に一致するものだけです）、ファイル全体を Rotproof の
  体裁（インデントは空白 2 つ）で書き直します。行末と byte order mark は保ちます。これらは人が書いたものを変えますが、
  どれもそのために実行するコマンドの中でのことです。Rotproof は記録を動かしたり書き換えたりしません。1.0 からは
  作業の記録を読まず、古いプロジェクトの記録は、あった場所にそのまま残ります。knowledge の文書をコードに照らして
  読み直したら、`rotproof follows`（キーを名指す）と `rotproof approve --reviewed`（人が端末で）が、その `follows` に
  ハッシュを書きます。読み直すのは人かエージェントで、固定はそれを記録するだけです。`rotproof approve --prune` は、
  どの禁じられた import にも合わない承認を取り除きます。`rotproof check` はそういう承認があると失敗します。
  取り除くだけなので、前に落ちていたものが通ることはありません。Rotproof はコードを動かしません。層の表が変わるとき
  （0.4.0、1.0.0-rc.3）、`rotproof init` は各層のコードの行き先を言い、動かすのは人です。
- **どの規則にも、失敗しうる検査がある。** 検査の無い規則はただの札で、すぐにずれます。
- **検査するものが無い検査は、失敗する。** 空の木で通る検査は、何も守りません。
- **規則は道具と一緒に届く。** プロジェクトは Rotproof の版を 1 つ固定し、規則の改善は、アップグレードという
  それだけのコミットで取り込みます。
- **どの言語でも同じ構造。** Rotproof は言語のランタイムを必要としない 1 つのバイナリなので、Rust や TypeScript の
  プロジェクトも、Python のプロジェクトと同じ層と記録を保ちます。
- **いまの状態を知りたい人は、すべてのファイルではなく、knowledge の索引を読む。** 索引は生成するもので、
  手では書きません。未完了の作業は Issue にあります。

## 仕様、開発、ライセンス

- **仕様:** [SPECIFICATION.md](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md)（英語）。コマンド、
  アップグレード、禁じられた import の承認、層、記録、`rotproof check` が検査するすべての規則があります。
- **開発とリリース:** 英語版の
  [Development](https://github.com/secta113/rotproof/blob/main/README.md#development) を参照してください。
- **ライセンス:** [MIT](https://github.com/secta113/rotproof/blob/main/LICENSE-MIT) または
  [Apache-2.0](https://github.com/secta113/rotproof/blob/main/LICENSE-APACHE) のどちらかを選べます。バイナリに含まれる
  crate はそれぞれのライセンスに従い、その文面は
  [THIRD-PARTY-LICENSES.txt](https://github.com/secta113/rotproof/blob/main/THIRD-PARTY-LICENSES.txt) にあります。
