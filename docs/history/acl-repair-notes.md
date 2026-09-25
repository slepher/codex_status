# ACL 修复：让沙箱能改写/删除工作区文件

> 背景与实测记录见 `PROGRESS.md`。**2026-09-25 的归档只失败在两个目录**，原因查清后是**两种**，
> 不要混为一谈。`tools/repair-acl.ps1` 曾存在但已丢失（`tools/` 目录在 DSH 路径下已不存在），
> 所以修复步骤写在这里，需要时在**提权** PowerShell 里粘这段。

## 症状与判据

shell 里对某些已存在的文件报 "Access to the path … is denied"，而**同目录新建文件却可以**。
用 SDDL 一眼判定：

```powershell
(Get-Acl 'project-workflow\live-template-delivery\plan.md').Sddl
```

- 能改的文件 SDDL 里有 `(A;ID;0x110156;;;S-1-4-1018769461-493222538)`
  → 这是沙箱的**能力 SID** 的 ACE（`0x110156` = `DeleteSubdirectoriesAndFiles, Write, Delete, Synchronize`）。
- 不能改的文件**没有**这条。它只有 `CodexSandboxUsers` / `Authenticated Users` / `Users` 等**组**的继承 ACE，
  而 DSH 沙箱用受限令牌，宽泛组不生效 → 只看那条能力 ACE。

所以"能不能写"是**逐对象 DACL**问题，不是路径策略，也不是"所有者"问题。

## 两种不同的原因（2026-09-25 实测）

| 目录 | 所有者 | SDDL 里缺能力 ACE | 原因 |
|---|---|---|---|
| `project-workflow/generic-display-platform-design/`（3 文件） | `喵的问都死\CodexSandboxOffline`（SID `…-1008`） | 是 | **属主 + DACL 都错**：早期沙箱账号建的目录 |
| `project-workflow/live-template-delivery/`（6 文件） | `喵的问都死\cogic` | 是 | **只有 DACL 错**：属主正常，仅缺能力 ACE |

**存量规律**：能力 ACE 是后加到仓库根的，**可继承 ACE 不会回溯传播**到已存在的对象。
因此 2026-09-25 时 `project-workflow/` 下仍有 **86 个文件**缺这条 ACE：

| 位置 | 缺 ACE 的文件数 |
|---|---|
| `generic-display-platform-implementation/` | 63 |
| `ble-rendezvous-power/` | 13 |
| `live-template-delivery/` | 6 |
| `generic-display-platform-design/` | 3 |
| `next-execution-plan-2026-09-23.md` | 1 |

## 修复脚本（在**提权**的 PowerShell 里运行）

要点：**必须走 .NET**。`icacls /grant "*S-1-4-…"` 会因无法映射账户名报
`No mapping between account names and security IDs`。**不要递归整棵树**
（`artifacts/` 有 7 万+ 文件，递归会拖死/中断）；只对目标目录加**可继承** ACE 就能新建文件，
只有需要"改写已存在文件"时才逐个改那个文件。

```powershell
# 0) 改这两个值：能力 SID 用 dsh 当前会话的（查法见下），$root 指向仓库
$capSid = 'S-1-4-1018769461-493222538'
$root   = 'D:\Documents\PlatformIO\Projects\codex_status'

# 能力 SID 的查法：在受限会话里看仓库根 SDDL，找 S-1-4- 开头的那个
#   (Get-Acl $root).Sddl

$targets = @(
  "$root\project-workflow\generic-display-platform-design",
  "$root\project-workflow\live-template-delivery",
  "$root\project-workflow\ble-rendezvous-power",
  "$root\project-workflow\generic-display-platform-implementation"
)

$sid = New-Object System.Security.Principal.SecurityIdentifier($capSid)
$me  = New-Object System.Security.Principal.SecurityIdentifier(
         [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value)

foreach ($t in $targets) {
    if (-not (Test-Path $t)) { "skip (missing): $t"; continue }

    # 1) 属主改回当前用户（CodexSandboxOffline 属主会让后续授权更难）
    $acl = Get-Acl $t
    $acl.SetOwner($me)

    # 2) 目录上加可继承的能力 ACE（只对目录，不递归）
    $rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
              $sid, 'Write,Delete,DeleteSubdirectoriesAndFiles,Synchronize',
              'ContainerInherit,ObjectInherit', 'None', 'Allow')
    $acl.AddAccessRule($rule)
    Set-Acl -Path $t -AclObject $acl
    "dir  OK: $t"

    # 3) 已存在文件逐个补 ACE（这些文件不会回溯继承）
    Get-ChildItem $t -Recurse -File | ForEach-Object {
        try {
            $fa = Get-Acl $_.FullName
            $fa.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule(
                $sid, 'Write,Delete,Synchronize', 'None', 'None', 'Allow')))
            Set-Acl -Path $_.FullName -AclObject $fa
        } catch { "  file FAIL: $($_.FullName) — $($_.Exception.Message)" }
    }
}
```

## 修复后要做的两件事

1. `git mv project-workflow/generic-display-platform-design docs/history/workflow/`
   和 `git mv project-workflow/live-template-delivery docs/history/workflow/`
   ——这两个目录的归档判定在 `docs/roadmap/archive-digest-legacy.md` 里已经是"可归档（只读历史）"。
2. 把 `docs/README.md` 的"仍在 `project-workflow/` 的专项"表和 `docs/roadmap/backlog.md` 的
   **D11** 一起更新（去掉这两个目录，或把 D11 标为已解决）。

## 不要做的事

- **不要用 `git stash` 保护现场**：它先 unlink 再写回，遇到缺 ACE 的文件会"删掉却写不回来"。
  2026-09-25 已经因此丢了 16 个文件（索引里有快照才无损恢复）。见 `PROGRESS.md`。
- 不要在沙箱里先建目录再改属主；新目录要按 `AGENTS.md` 工作流约定**提权创建**。
