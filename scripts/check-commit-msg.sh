#!/usr/bin/env bash
# 校验提交信息是否符合 MyDeployMate-Tauri/docs/提交规范.md。
#
# 用法：
#   scripts/check-commit-msg.sh --file <消息文件>     # commit-msg 钩子
#   scripts/check-commit-msg.sh --range <base> <head> # 检查 base..head 的所有非合并提交
#   scripts/check-commit-msg.sh --title "<PR 标题>"   # 只检查一行标题
#   scripts/check-commit-msg.sh                       # 等同于 --range origin/main HEAD
set -euo pipefail
export LC_ALL=C.UTF-8

TYPES='feat|fix|refactor|perf|test|docs|style|build|ci|chore|revert'
SCOPES='core|store|exec|tauri|ui|design|spike|test-env|demo|deps|repo'
MAX_HEADER=72

failures=0

# check_message <标签> <完整消息>
check_message() {
  local label=$1 msg=$2 header second errs=()
  # 去掉 git 注释行和末尾空行
  msg=$(printf '%s\n' "$msg" | sed '/^#/d')
  header=$(printf '%s\n' "$msg" | sed -n '1p')
  second=$(printf '%s\n' "$msg" | sed -n '2p')

  # git 自动生成的合并与回滚标题直接放行
  if [[ $header =~ ^Merge\  || $header =~ ^Revert\ \" ]]; then
    return 0
  fi

  if [[ $header =~ ^(fixup|squash|amend)! ]]; then
    errs+=("fixup!/squash! 提交必须在合并前 rebase 掉")
  elif [[ ! $header =~ ^($TYPES)(\(([a-z0-9-]+(,[a-z0-9-]+)*)\))?(!)?:\ (.+)$ ]]; then
    errs+=("标题格式应为 'type(scope): subject'，type 取 ${TYPES//|/\/}")
  else
    local scopes=${BASH_REMATCH[3]} subject=${BASH_REMATCH[6]} s
    if [[ -n $scopes ]]; then
      IFS=',' read -ra list <<<"$scopes"
      for s in "${list[@]}"; do
        [[ $s =~ ^($SCOPES)$ ]] || errs+=("未知 scope '$s'，可选：${SCOPES//|/, }")
      done
    fi
    [[ $subject =~ ^[[:space:]] ]] && errs+=("冒号后只能有一个空格")
    [[ $subject =~ [.。]$ ]] && errs+=("subject 结尾不要加句号")
  fi

  (( ${#header} > MAX_HEADER )) && errs+=("标题 ${#header} 字符，超过 $MAX_HEADER")
  [[ -n $second ]] && errs+=("标题与正文之间需要一个空行")

  if (( ${#errs[@]} )); then
    failures=$((failures + 1))
    printf '✗ %s\n    %s\n' "$label" "$header"
    printf '    - %s\n' "${errs[@]}"
  else
    printf '✓ %s  %s\n' "$label" "$header"
  fi
}

mode=${1:---range}
case $mode in
  --file)
    check_message "commit" "$(cat "$2")"
    ;;
  --title)
    check_message "title" "$2"
    ;;
  --range)
    base=${2:-origin/main}
    head=${3:-HEAD}
    shas=$(git rev-list --no-merges --reverse "$base..$head")
    if [[ -z $shas ]]; then
      echo "范围 $base..$head 内没有需要检查的提交"
      exit 0
    fi
    for sha in $shas; do
      check_message "${sha:0:8}" "$(git log -1 --format=%B "$sha")"
    done
    ;;
  *)
    sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac

if (( failures )); then
  echo
  echo "有 $failures 条提交信息不符合规范，见 MyDeployMate-Tauri/docs/提交规范.md"
  exit 1
fi
