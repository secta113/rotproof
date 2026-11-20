# Rotproof

[English](https://github.com/secta113/rotproof/blob/main/README.md) | 日本語

<!-- Translated from README.md at 69971a81. `cargo xtask ci` fails when README.md is another text: bring this file to
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
  コードを分けさせる手段でもあります。例外は、承認のファイルにある、理由つきのエントリだけで、レビューする人は差分でそれを見ます。
- **モノレポの各部分が、自分の層を守ります。** モノレポのパッケージやモジュラーモノリスのモジュールをユニットとして
  宣言すると、そのあいだの import がすべて検査されます。ある部分が別の部分に届くのは、届いてよい層を通るときだけです。
  誰も必要としないものを集めてしまう共有ユニットは、差分に出る一覧で見つかります（「ユニット」を参照）。
- **内側の層は I/O をしません。** `domain`、`protocol`、`application`、`presentation` は、ファイル、ネットワーク、
  プロセス、端末に触れるものを import せず、時計、乱数、コマンドライン引数も直接は読みません。Python と TypeScript
  では、そのうえで宣言が許したパッケージだけを import します。I/O は `infrastructure` が、ポートの向こうで行います
  （[The inner layers do no I/O](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#the-inner-layers-do-no-io)
  を参照）。
- **マニフェストが、コードの import と食い違いません。** Python のユニットの `pyproject.toml` と TypeScript のユニットの
  `package.json` を、何もインストールせずに import と突き合わせます。import しているのに書いていないパッケージは失敗し、
  どのコードも import しない実行時の依存も失敗します（Rust は、書いていない crate を Cargo が止めるので、crate の
  `[dependencies]` の使われていないものを見ます）。Python と TypeScript では、層のコードが使えるのは実行時の依存だけで、
  テストは両方を使えます（Python では `tests/` のファイル、TypeScript では Vitest と Jest が既定で拾う `*.test.*`、
  `*.spec.*`、`__tests__/` の中のファイルで、`rotproof check` が毎回その数を表示します）。import 名と配布名の対応（`dotenv` は `python-dotenv`）と、import せずに使う依存は宣言に書き、
  判定はどの環境でも同じです。
- **Python のプロジェクトはロックされ、仮想環境は 1 つです。** 依存は `pyproject.toml` に書き（`requirements.txt` は
  数えません）、ロックファイル（`uv.lock`、`poetry.lock`、`pdm.lock`、`pylock.toml`）をコミットします。たまたまある
  マシンに入っているパッケージで import が通ることがなくなります。ディスク上の仮想環境は、ロックファイルの隣の `.venv`
  だけで、`.gitignore` が除外していなければなりません。プロジェクトのディレクトリを、ignore されたものも含めて探し、
  `venv/` などは失敗します。`rotproof create` は、`dev` グループに Rotproof を固定した `pyproject.toml` と、プロジェクトの
  ロックファイルの道具に合わせた CI のワークフロー（まだ無いときは uv の形）を書きます。
- **コードからは読み取れないことが、正しいまま残ります。** コードの背後にある理由は knowledge の文書に書き、文書は
  自分が説明するコードを名指して、ハッシュで固定します。そのコードが変わると、誰かが文書をコードに照らして読み、
  固定し直すまで `rotproof check` が失敗します。
- **先送りした作業が、コメントに隠れません。** コードのコメントの `TODO`、`FIXME`、`XXX`、`HACK`、`NOTE` は検査を
  失敗させます。見つけたことは、いま直すか、Issue に書くか、手放すかのどれかです。`todo_link` を宣言したプロジェクト
  では、追っている Issue を名指す `TODO` は残せて、毎回の検査がそれを一覧に出します。型検査を止めるコメントも
  通りません。環境変数を読めるのは `infrastructure` だけです。
- **バイナリしか持たないエージェントも、迷いません。** `rotproof --help` が始める順番を示します。規則はプロジェクトの
  スタックに合わせて `.rotproof/AGENTS.md` に書かれます。エージェントにそれを読ませるかは、あなたが決めます。
  `rotproof create` が問いを出し、エージェントが最初に読むファイルに足す 1 行を示します。足すか断るまで
  `rotproof check` は失敗します。Rotproof の上げ方と、コマンドをいつ走らせるかは `.rotproof/running.md` にあり、
  ガイドがそれを名指します。
- **規則は、固定した版のものです。** プロジェクトは Rotproof の版を 1 つ、厳密に固定します。規則が変わるのは、
  プロジェクト自身のコミットの中だけです（「互換性」を参照）。

Rotproof がやらないこと: `rotproof check` は報告するだけで、何も直しません。文書がいまも正しいかどうかは判断できず、
分かるのは、その下のコードが変わったことだけです。読み直すのは、人か、変更を加えたエージェントです。

## インストール

Rotproof は Windows x86_64、Linux x86_64（glibc 2.17 以上）、macOS arm64（Apple silicon、macOS 11 以上）向けに
出しています。版は厳密に固定してください。アップグレードは新しい規則を持ち込みうるので、それだけのコミットにするためです。

```sh
pip install rotproof==2.0.0-rc.1
```

Node のプロジェクトでは npm で固定します。プラットフォームに合ったバイナリも一緒に入ります（パッケージ `rotproof` と、
`@rotproof/linux-x64`、`@rotproof/win32-x64`、`@rotproof/darwin-arm64` のいずれか）。

```sh
npm install -D -E rotproof@2.0.0-rc.1
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
あること（仮想環境で `pip install` すれば入ります）と、依存をロックする 1 つの手順のための
[uv](https://docs.astral.sh/uv/)（Poetry や PDM でもロックできます）と、テキストエディタが要ります。プロジェクトは
Git のリポジトリでなくてもかまいません。プロジェクトのコードは実行しません。出力は、これを書いたときに Rotproof が
実際に出したもので、省いた行は `...` にしてあります。

### 1. プロジェクトを作る

```sh
mkdir shop
cd shop
rotproof init --stack python --ui-design none
```

```text
wrote .config/rotproof.toml
next: declare in absent the layers this project does not have, then run `rotproof create`, which lists the layers it would make, and `rotproof create --yes` to make them
```

`init` が書くのは宣言 `.config/rotproof.toml` だけです。これが、このプロジェクトが宣言する構造です。スタックと、
画面の無いプロジェクト（コマンドラインの道具、ライブラリ、サーバー）のための `--ui-design none` が入っています。`absent` には
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
not written: the lock file (uv.lock, poetry.lock, pdm.lock or pylock.toml), which a tool makes from pyproject.toml (`uv lock`): until one is committed, `rotproof check` fails
ask the person: should this project's agent read the rules Rotproof keeps, in .rotproof/AGENTS.md? The person decides; an agent asks before it adds the line or declines
  yes: add this line to the file the agent reads first (there is no AGENTS.md or CLAUDE.md: ask the person which file to make):
    The rules Rotproof keeps: @.rotproof/AGENTS.md
  no: add "guide-pointer" to declined in .config/rotproof.toml
```

層はそれぞれディレクトリで、`__init__.py` にその役割が書いてあります。`docs/` には記録が、`.rotproof/AGENTS.md` には
スタックに合わせて書かれた規則が入ります。残りは `rotproof check` を実行する CI のワークフローです。

Rotproof は `AGENTS.md` も `CLAUDE.md` も書きません。エージェントに何を伝えるかは、あなたが決めることです。そのため
`create` は最後に問いを出し、答えるまで終了コード 2 で終わります。エージェントに規則を守らせるなら、エージェントが最初に
読むファイル（Claude Code なら `CLAUDE.md`、ほかの多くは `AGENTS.md`）に、その 1 行を書きます。

```sh
echo "The rules Rotproof keeps: @.rotproof/AGENTS.md" > AGENTS.md
```

Python のプロジェクトはロックされます。依存は `pyproject.toml` に書き
（`create` は、`[dependency-groups]` の `dev` に Rotproof を固定して書きました）、ロックファイルをコミットします。
ロックファイルは道具にしか作れないので、それまで `rotproof check` は失敗します。

```sh
rotproof check
```

```text
a Python project lists its dependencies in pyproject.toml, commits a lock file, and keeps its virtual environment in .venv beside it:
  pyproject.toml has no lock file in the root or above it: commit one (uv.lock, poetry.lock, pdm.lock or pylock.toml), made by the tool from pyproject.toml, so that every machine installs the same packages (a lock file a .gitignore excludes is not committed)
```

ロックすれば、ほかに何も書かなくても最初の検査に通ります。

```sh
uv lock
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

`rotproof new` で `docs/knowledge/rates.md` を始めます。書き間違えやすい frontmatter を書き、その下には何も書きません。

```sh
rotproof new rates
```

```text
wrote docs/knowledge/rates.md (area: pricing)
```

続けて、本文を書くために尋ねることを表示します。なぜそうなっているか、何を考えてやめたか、本来どう動くべきか、コードの外で
知っておかなければならないことは何か、です。`description` は空のままなので、書くまで `rotproof check` は失敗します。
description と本文を書き、title を変えます。

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

タスクが終わったら、`rotproof log` がその記録を `docs/log.md` に足します。今日の日付を時計から読み、その日の見出しの
いちばん上に書きます。見出しが無ければ作ります。

```sh
rotproof log --title "Round the converted price" --issue acme/shop#12 --problem None \
  --lesson "The rate is passed in, so a test of convert needs no feed"
```

```text
wrote docs/log.md: the entry is under ## 2026-11-16
next: run `rotproof check`
```

`--problem` と `--lesson` はどちらも必須で、書くことが無いときは `None` と書きます。どちらも繰り返すと一覧になります。

### ここから先

プロジェクトをコミットしてください。作られた CI のワークフローは push のたびに `rotproof check` を実行し、
`pyproject.toml` の `dev` グループ（ロックファイルがロックします）は Rotproof を厳密な版に固定しています。`rotproof check` が検査することの詳細、コマンド、
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
  `infrastructure`、`application`、`protocol`、`domain` と、Python と TypeScript では画面のための `ui`、
  `presentation`）。この層に
  分けられないコードは責務が混ざっているので、層は LLM や人にコードを分けさせる手段です。どのコードも、役割と
  許される import が書かれた層のどれかに収まらなければなりません。Rotproof はプロジェクトの開始時に層を作り、
  実行のたびに検査するので、依存の向きは意図したとおりに保たれます。層をそれぞれ持つ複数の部分からなるプロジェクトは、
  それらをユニットとして宣言します（「ユニット」を参照）。画面のある Python のプロジェクトでは、ビューをサーバー側の
  テンプレート（Jinja、Django のテンプレート）で書くこともできます。テンプレートは `ui` の段に置き、
  `infrastructure.inbound` のルートがそれを描画し、テンプレートどうしの参照（`{% extends %}`、`{% include %}`、
  `{% import %}`、`{% from %}`）は import と同じように判定します。どのテンプレート言語のどの版として読んだかは
  テンプレートを読んだ実行のたびに表示し、プロジェクトの `pyproject.toml` が別の版を求めているときは判定しません
  （[Views that are templates](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#views-that-are-templates) を参照）。
- **記録:** いまどうなっているか、なぜそうなのかのうち、コードからは読み取れないこと。これは、説明するコードを追う
  knowledge の文書に書き、その下のコードが変わると `rotproof check` が失敗します（目にしたときだけ意味を持つ
  ことのための alarm もあります）。もう 1 つは、各タスクが残したものの log です。未完了の作業は、ここではなく
  Issue に置きます。知識も、見つけたことも、記録の外には残しません。コードのコメントの `TODO` や `NOTE` は検査を
  失敗させます。ただし、`todo_link` を宣言したプロジェクトで、Issue を名指す `TODO` は別です。

## ユニット

プロジェクトは、何も言わなければ 1 つのユニットで、層はスタックが置く場所にある 1 組です。モノレポのパッケージや
モジュラーモノリスのモジュールのように、層をそれぞれ持つ複数の部分からなるプロジェクトは、各部分をユニットとして、
`.config/rotproof.toml` に `[[unit]]` テーブルを 1 つずつ宣言します。すると Rotproof は、各ユニットの中で層を保ち、
ユニットのあいだの import も判定します。各ユニットは持たない層を自分の `absent` に書くので、先頭の `absent` は空の
ままです。`rotproof init` は宣言の末尾に、コメントにした例を書きます。共有ユニット 1 つ、共有でないユニット 2 つ、
アプリ 1 つの Python のモノレポは、次のようになります。

```toml
[[unit]]
name = "core"
path = "packages/core/src/core"       # where its layers sit directly
root = "packages/core"                # where its manifest is (`path` when left out)
shared = true                         # any unit may import it

[[unit]]
name = "billing"
path = "packages/billing/src/billing"
root = "packages/billing"

[[unit]]
name = "shipping"
path = "packages/shipping/src/shipping"
root = "packages/shipping"

[[unit]]
name = "cli"
path = "apps/cli/cli"
root = "apps/cli"
entry = ["apps/cli/__main__.py"]        # an app: its composition wires the other units
absent = ["application", "protocol", "domain"]   # an app holds only composition and infrastructure
```

`rotproof create --yes` は、各ユニットの層と、足りない entry ファイルと、足りない各ユニットのマニフェスト（`root` に置く、
ユニットの名前で依存の無い `pyproject.toml`）を作ります。同じ検査が各ユニットの中で働き、
指摘にはユニットの名前が付きます。ユニットのあいだの規則は少数です。

- ユニットは、自分のユニットの層を表のとおりに import し、共有でない限り、ほかのユニットは import しません。
- 共有ユニットが import するのは共有ユニットだけで、循環はしません。層は、共有ユニットの層のうち、自分の層が表で
  import してよいものを import します（`infrastructure.inbound` は共有ユニットの `protocol.inbound` を import します）。
  ただし `application` は、ほかのユニットの `application` を import しません。ユースケースは渡されます。追加は 1 つで、
  `protocol.inbound` は共有ユニットの `domain` も import します。ユニットをまたいで使う値の型はそこにあるので、入力
  ポートはその形を写さずに、名前で指せます。
- `composition` は、ほかのユニットをそのユニットの `protocol` を通して import し、共有ユニットなら `domain`（ポートが
  取る値の型）も import します。共有ユニットも含め、各ユニットは自分の中身を自分の `composition` で組み立てるから
  です。アプリの `composition` はユニットを組み合わせるので、ユニットの `composition` も import しますが、別の
  アプリのものは import しません。entry ファイルが import するのは、自分のユニットの `composition` だけです。
- ほかのユニットの import は、その層のモジュールを名指し、ユニット全体は名指しません。層の外のコードから何を読むかは
  見えないからです。

共有ユニットはどのユニットからも import できるので、誰も必要としないものを集めても止められません。共有ユニットを
宣言したプロジェクトは `.rotproof/readers.md` を持ちます。各ユニットが各共有ユニットの何を読むかを名前で並べ、
読むユニットが 1 つだけの名前は別にしてあります。`rotproof index` が import から書き、古くなっている間は
`rotproof check` が失敗します。一覧に出たことで失敗する規則はありません。1 つのユニットだけが使うライブラリを、
わざと共有にすることもあるからです。一覧は読む人のためにあり、名前が現れたり読み手が変わったりしたことを差分で
見せます。

コードがなぜこのユニットに分かれているかは、人が言うことです。ユニットを宣言していて、タグにする area があり、
ユニットを追う文書がまだ無いプロジェクトで `rotproof init` を実行すると、`docs/knowledge/units.md` を一度だけ下書きします。
ユニットと、コードにあるとおりのユニット間の import を Mermaid の図にし、各ユニットの節には責務を書く欄が残して
あります。この文書は `.config/rotproof.toml::unit` を追うので、ユニットを足す、消す、変えると、文書を宣言に照らして
読み直し、固定し直すまで `rotproof check` が失敗します。すべての規則と、スタックごとのユニットの import の
仕方は、[Units](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#units) にあります。

## 原則

- **宣言が正であり、Rotproof は直さない。** プロジェクトが宣言した構造が、その構造です。それと違う木は、どちら向きの
  違いでも失敗し、木と宣言のどちらが間違っているかは誰かが決めます。`rotproof check` は報告するだけです。書くのは、
  人やエージェントが実行するコマンドで、何をしたかを言います。そういうコマンドは、足りないものを足し、人が書いた値は
  そのままにします。足すのは、無い層やプロジェクトのファイル、宣言に無い欄、承認、`rotproof init` の
  名前の付いた更新が持ち込むもの
  （[Upgrading Rotproof](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#upgrading-rotproof) を参照）、
  そして誰も手で書かない、Rotproof が生成するファイルです。宣言に無い欄は、最初の値を一度だけ木から取ります
  （`areas` には knowledge の文書が使っているタグが入るので、規則を守っていた文書は守ったままになります）。それ以降、
  宣言に無いタグを持つ文書は失敗します。`.claude/settings.json` の更新は、キーとその順番と値を保ちますが
  （取り除くのは、Rotproof が書いた stop hook と time hook と deny の規則に厳密に一致するものだけです）、ファイル全体を Rotproof の
  体裁（インデントは空白 2 つ）で書き直します。行末と byte order mark は保ちます。Rust のプロジェクトでは、
  `rotproof create` がルートの `Cargo.toml` の `members` を変え、ほかは変えません。作った crate とパッケージを足し、
  ユニットより前に Rotproof が書いた glob `crates/*` がユニットのディレクトリに当たるようになったら、それまで当たっていた
  crate の列に置き換えます。これらは人が書いたものを変えますが、
  どれもそのために実行するコマンドの中でのことです。Rotproof は記録を動かしたり書き換えたりしません。1.0 からは
  作業の記録を読まず、古いプロジェクトの記録は、あった場所にそのまま残ります。knowledge の文書をコードに照らして
  読み直したら、`rotproof follows`（キーを名指す）が、その `follows` にハッシュを書きます。読み直すのは人かエージェントで、固定はそれを記録するだけです。`rotproof new` は、書き間違えやすい frontmatter と、
  check が失敗する空の description だけを書いて文書を始め、本文は書きません。`rotproof log` は、今日の日付を時計から読み、
  その日の見出しの下に記録を足します。`rotproof approve --prune` は、
  どの禁じられた import にも合わない承認を取り除きます。`rotproof check` はそういう承認があると失敗します。
  取り除くだけなので、前に落ちていたものが通ることはありません。`rotproof baseline` は、今ある違反を一度だけ書きます。
  `rotproof check` はそれを通して件数を数え、その `--prune` も、エントリを取り除くだけです。Rotproof はコードを動かしません。層の表が変わるとき
  （0.4.0、1.0.0-rc.3）、`rotproof init` は各層のコードの行き先を言い、動かすのは人です。
- **どの規則にも、失敗しうる検査がある。** 検査の無い規則はただの札で、すぐにずれます。
- **検査するものの置き場が無い検査は、失敗する。** 層や `docs/knowledge/` の無い木で通る検査は、何も守りません。
  置き場があって中身がまだ空の新しいプロジェクトは、最初の check を通ります。
- **規則は道具と一緒に届く。** プロジェクトは Rotproof の版を 1 つ固定し、規則の改善は、アップグレードという
  それだけのコミットで取り込みます。
- **どの言語でも同じ構造。** Rotproof は言語のランタイムを必要としない 1 つのバイナリなので、Rust や TypeScript の
  プロジェクトも、Python のプロジェクトと同じ層と記録を保ちます。
- **いまの状態を知りたい人は、すべてのファイルではなく、knowledge の索引を読む。** 索引は生成するもので、
  手では書きません。未完了の作業は Issue にあります。

## 仕様、開発、ライセンス

- **仕様:** [SPECIFICATION.md](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md)（英語）。コマンド、
  アップグレード、禁じられた import の承認、層、ユニット、記録、`rotproof check` が検査するすべての規則があります。
  README に出てこない `rotproof guide`、`approved_max`、`unchecked`、`moving`、`without_import`、`stale_after`、
  プロジェクトの `AGENTS.md` の地図も、そこにあります。
- **開発とリリース:** 英語版の
  [Development](https://github.com/secta113/rotproof/blob/main/README.md#development) を参照してください。
- **ライセンス:** [MIT](https://github.com/secta113/rotproof/blob/main/LICENSE-MIT) または
  [Apache-2.0](https://github.com/secta113/rotproof/blob/main/LICENSE-APACHE) のどちらかを選べます。バイナリに含まれる
  crate はそれぞれのライセンスに従い、その文面は
  [THIRD-PARTY-LICENSES.txt](https://github.com/secta113/rotproof/blob/main/THIRD-PARTY-LICENSES.txt) にあります。

## 謝辞

Rotproof は、David Thomas と Andrew Hunt の『達人プログラマー 第二版 熟達に向けたあなたの旅』（原書
*The Pragmatic Programmer, 20th Anniversary Edition*）から生まれました。もとになったのは「ソフトウェアのエントロピー」
（割れた窓を放置しない）、「直交性」、「DRY原則 - 二重化の過ち」です。リポジトリは、放置された窓ひとつずつから
腐っていきます。Rotproof は、LLM と人がコードを変えていくあいだ、窓が割れるたびに、毎回同じやり方でそれを見つける
ための道具です。
