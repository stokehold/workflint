# PowerShell completion for workflint. `workflint completion powershell` prints it.

Register-ArgumentCompleter -Native -CommandName workflint, workflint.exe -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $flags = @(
        @{ Name = 'help'; Values = 'Bool'; Help = 'Show the usage' }
@TABLE@    )
    $formats = @(@FORMATS@)
    $result = {
        param($text, $help)
        [System.Management.Automation.CompletionResult]::new($text, $text, 'ParameterValue', $help)
    }

    # The words before the one under the cursor, without the command.
    $words = @($commandAst.CommandElements |
        Where-Object { $_.Extent.EndOffset -lt $cursorPosition } |
        Select-Object -Skip 1 |
        ForEach-Object { $_.Extent.Text })

    if ($words.Count -ge 1 -and $words[0] -eq 'completion') {
        if ($words.Count -eq 1) {
            @(@SHELLS@) | Where-Object { $_ -like "$wordToComplete*" } |
                ForEach-Object { & $result $_ 'shell' }
        }
        return
    }

    # Go's flag package ends the flags at `--` and at the first argument that is not a flag.
    $i = 0
    if ($words.Count -ge 1 -and $words[0] -eq 'check') { $i = 1 }
    $flag = $null
    for (; $i -lt $words.Count; $i++) {
        $word = $words[$i]
        if ($word -eq '--' -or $word -eq '-' -or -not $word.StartsWith('-')) { return }
        if ($word.Contains('=')) { continue }
        $name = $word -replace '^--?', ''
        $flag = $flags | Where-Object { $_.Name -eq $name -and $_.Values -ne 'Bool' }
        if ($flag) {
            if ($i + 1 -eq $words.Count) { break }
            $i++
            $flag = $null
        }
    }

    # The value of a flag: after the flag, or after `=` in the same word.
    $prefix = ''
    $value = $wordToComplete
    if (-not $flag -and $wordToComplete -match '^(--?([^=]+)=)(.*)$') {
        $prefix = $Matches[1]
        $value = $Matches[3]
        $flag = $flags | Where-Object { $_.Name -eq $Matches[2] }
        if (-not $flag) { return }
    }
    if ($flag) {
        $candidates = switch ($flag.Values) {
            'Format' { $formats }
            'Bool' { @('true', 'false') }
            'Command' { Get-Command -CommandType Application -Name "$value*" -ErrorAction Ignore | ForEach-Object { $_.Name } }
            default { $null }
        }
        if ($null -eq $candidates) { return }
        $candidates | Where-Object { $_ -like "$value*" } | Sort-Object -Unique |
            ForEach-Object { & $result "$prefix$_" $flag.Help }
        return
    }

    if ($wordToComplete.StartsWith('-')) {
        $dashes = if ($wordToComplete.StartsWith('--')) { '--' } else { '-' }
        $flags | Where-Object { "$dashes$($_.Name)" -like "$wordToComplete*" } |
            ForEach-Object { & $result "$dashes$($_.Name)" $_.Help }
        return
    }
    if ($words.Count -eq 0) {
        @(@COMMANDS@) | Where-Object { $_ -like "$wordToComplete*" } |
            ForEach-Object { & $result $_ 'command' }
    }
    # Nothing more: PowerShell completes file names.
}
