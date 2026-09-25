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

## 只改所有者（最小修复，推荐先做这一步）

**实测：DSH 会话里改不了所有者，连 `danger-full-access` 提权也不行。** 2026-09-25 试了三次：

```
SetOwner FAILED: Attempted to perform an unauthorized operation.
elevated?     False          ← 提权批准后依然是 False
current SID:  S-1-5-21-341968838-3967994556-1780607818-1001
```

**关键结论：沙箱提权只放宽"路径写入"限制，不会把令牌变成管理员**，因此 `SeRestorePrivilege`
永远拿不到。所以这一步只能由你在管理员终端执行。（`elevated? False` 是判据：只要它是 False，
改属主就一定失败。）

### 同一次实测得到的其余事实（有用）

把"改属主 + 补 ACE"放在同一个循环里跑，失败了 64 项、成功 90 项，**失败集合与"属主不是 cogic"
的集合完全重合**：

| 目录 | 总对象 | 属主非 cogic | 缺 ACE | 结果 |
|---|---|---|---|---|
| `generic-display-platform-design` | 4 | **4** | 4 | 全失败 |
| `generic-display-platform-implementation` | 95 | **55** | 55 | 属主非 cogic 的 55 个全失败，其余 40 个补 ACE 成功 |
| `ble-rendezvous-power` | 14 | **4** | 4 | 4 个失败，其余 10 个补 ACE 成功 |
| `live-template-delivery` | 7 | 1 | 1 | 只有目录本身失败（它属主是 `CodexSandboxOffline`），6 个文件补 ACE 成功 |

推论：
1. **属主是闸门。** 属主是 `CodexSandboxOffline` 的对象，我的令牌既改不了属主也改不了 DACL。
2. **属主是 cogic 但缺 ACE 的对象，DACL 是可以改的**（沙箱令牌带 `WRITE_DAC`），所以补 ACE
   那一步能在沙箱内完成——90 个对象已经补好了。
3. 因此在管理员终端里跑下面这段时，**真正必须做的只有"改属主"**；补 ACE 可以留着，
   重复执行也无害（脚本会跳过已有 ACE 的对象）。

改所有者需要 `SeRestorePrivilege`，只有**提权**进程有。所以这一步必须由你自己执行。
在**管理员** PowerShell 里粘这段（复制粘贴即用，不含变量占位）：

```powershell
$root = 'D:\Documents\PlatformIO\Projects\codex_status'
$me   = New-Object System.Security.Principal.SecurityIdentifier(
          [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value)

"running as: $($me.Value)   elevated: " +
  ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
   ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

# 硬闸门：不是管理员就别继续，否则每个对象都报 "unauthorized operation"
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
          ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw '先以管理员身份重开 PowerShell（"以管理员身份运行"），否则改不了属主。'
}

# 只处理确实需要改的 4 个目录（其余 project-workflow 文件属主已经是 cogic）
$targets = @(
  "$root\project-workflow\generic-display-platform-design",
  "$root\project-workflow\live-template-delivery",
  "$root\project-workflow\ble-rendezvous-power",
  "$root\project-workflow\generic-display-platform-implementation"
)

# 顺带把仓库根的沙箱能力 SID 抓出来（不要写死，随机能力 SID 每次会话可能不同）
$capMatch = [regex]::Match((Get-Acl $root).Sddl, 'S-1-4-\d+(?:-\d+)+')
$capSid   = if ($capMatch.Success) { $capMatch.Value } else { $null }
"capability SID: $capSid"

foreach ($t in $targets) {
    if (-not (Test-Path $t)) { "skip (missing): $t"; continue }

    # 1) 目录本身：改属主
    $a = Get-Acl $t; $a.SetOwner($me); Set-Acl -Path $t -AclObject $a
    "owner set: $t"

    # 2) 目录上加可继承的能力 ACE —— 不递归，只动这一个目录对象
    if ($capSid) {
        $a = Get-Acl $t
        $rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
            (New-Object System.Security.Principal.SecurityIdentifier($capSid)),
            'Write,Delete,DeleteSubdirectoriesAndFiles,Synchronize',
            'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $a.AddAccessRule($rule); Set-Acl -Path $t -AclObject $a
        "ace  set: $t"
    }

    # 3) 已存在文件：既要改属主，也要补能力 ACE（可继承 ACE 不会回溯）
    Get-ChildItem $t -Recurse -File | ForEach-Object {
        try {
            $fa = Get-Acl $_.FullName
            $fa.SetOwner($me)
            if ($capSid) {
                $fa.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule(
                    (New-Object System.Security.Principal.SecurityIdentifier($capSid)),
                    'Write,Delete,Synchronize', 'None', 'None', 'Allow')))
            }
            Set-Acl -Path $_.FullName -AclObject $fa
        } catch { "  file FAIL: $($_.FullName) — $($_.Exception.Message)" }
    }
}

# 4) 复核：应全部为 cogic，且 SDDL 里都有能力 ACE
foreach ($t in $targets) {
    if (-not (Test-Path $t)) { continue }
    $bad = Get-ChildItem $t -Recurse -File |
           Where-Object { (Get-Acl $_.FullName).Owner -ne "$env:USERDOMAIN\$env:USERNAME" }
    "{0,-56} not-mine={1}" -f $t, $bad.Count
}
```

**为什么三步要一起做（而不是"只改属主"）**：受限令牌既没有 `SeRestorePrivilege`（改属主）也没有
`WRITE_DAC`（改 DACL，缺的正是那条 ACE），所以两者在沙箱里都做不到。而在**提权**进程里
`SeTakeOwnershipPrivilege` / `SeRestorePrivilege` 都在，第 1 步能成功；但只改属主**不会**让沙箱会话
恢复写入能力——沙箱仍用受限令牌，判定只看那条能力 ACE。所以第 2 步（目录 ACE）与第 3 步
（逐个已存在文件补 ACE，因为可继承 ACE 不回溯）必须跟上，否则改了属主照样写不进去。

修完后回到本文件的下一节，执行两个 `git mv` 并更新文档。

## 完整修复脚本（在**提权**的 PowerShell 里运行）

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
