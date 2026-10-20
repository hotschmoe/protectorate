package main

import (
	"fmt"
	"strings"
)

func (c *command) addBuiltins() {
	c.AddCommand(&command{Use: "help [command]", Short: "Help about any command", Long: "Help provides help for any command in the application.\nSimply type protectorate help [path to command] for full details.", RunE: func(cmd *command, _ []string) error { return cmd.Root().Help() }})
	completion := &command{Use: "completion", Short: "Generate the autocompletion script for the specified shell", Long: "Generate the autocompletion script for protectorate for the specified shell.\nSee each sub-command's help for details on how to use the generated script."}
	for _, shell := range []string{"bash", "fish", "powershell", "zsh"} {
		noDescriptions := false
		sub := &command{Use: shell, Short: "Generate the autocompletion script for " + shell, Long: completionHelp[shell], Args: noArgs, RunE: func(cmd *command, _ []string) error {
			_, err := fmt.Fprint(cmd.OutOrStdout(), completionScript(shell, !noDescriptions))
			return err
		}}
		sub.Flags().BoolVar(&noDescriptions, "no-descriptions", false, "disable completion descriptions")
		sub.omitFlagsInUsage = shell == "bash"
		completion.AddCommand(sub)
	}
	c.AddCommand(completion)
	for _, name := range []string{"__complete", "__completeNoDesc"} {
		c.AddCommand(&command{Use: name, Hidden: true, rawArguments: true, RunE: func(cmd *command, args []string) error {
			target := cmd.Root()
			prefix := ""
			if len(args) > 0 {
				prefix = args[len(args)-1]
				args = args[:len(args)-1]
			}
			for i := 0; i < len(args); i++ {
				if strings.HasPrefix(args[i], "-") {
					n, err := target.visibleFlags().parseOne(args[i:], false)
					if err != nil {
						return nil
					}
					i += n - 1
					continue
				}
				if next := target.child(args[i]); next != nil {
					target = next
				} else {
					break
				}
			}
			w := cmd.OutOrStdout()
			for _, child := range target.Commands() {
				if !child.Hidden && strings.HasPrefix(child.Name(), prefix) {
					if name == "__complete" {
						fmt.Fprintf(w, "%s\t%s\n", child.Name(), child.Short)
					} else {
						fmt.Fprintln(w, child.Name())
					}
				}
			}
			if strings.HasPrefix(prefix, "-") {
				target.visibleFlags().VisitAll(func(f *cliFlag) {
					if !f.Hidden && strings.HasPrefix("--"+f.Name, prefix) {
						if name == "__complete" {
							fmt.Fprintf(w, "--%s\t%s\n", f.Name, f.Usage)
						} else {
							fmt.Fprintln(w, "--"+f.Name)
						}
					}
				})
				if strings.HasPrefix("--help", prefix) {
					if name == "__complete" {
						fmt.Fprintln(w, "--help\thelp for "+target.Name())
					} else {
						fmt.Fprintln(w, "--help")
					}
				}
			}
			fmt.Fprintln(w, ":4")
			return nil
		}})
	}
}

func completionScript(shell string, descriptions bool) string {
	method := "__completeNoDesc"
	if descriptions {
		method = "__complete"
	}
	switch shell {
	case "bash":
		return `_protectorate() {
 local line
 COMPREPLY=()
 while IFS= read -r line; do
  [[ "$line" == :* ]] && continue
  COMPREPLY+=("${line%%$'\t'*}")
 done < <(protectorate ` + method + ` "${COMP_WORDS[@]:1:COMP_CWORD}")
}
complete -F _protectorate protectorate
`
	case "zsh":
		return `#compdef protectorate
_protectorate() {
 local -a candidates
 candidates=("${(@f)$(protectorate ` + method + ` "${words[@]:1:CURRENT-1}")}")
 candidates=("${(@)candidates:#:*}")
 compadd -- "${(@)candidates%%$'\t'*}"
}
compdef _protectorate protectorate
`
	case "fish":
		return `function __protectorate_candidates
 set -l words (commandline -opc)
 protectorate ` + method + ` $words[2..-1] (commandline -ct) | string match -rv '^:'
end
complete -c protectorate -f -a '(__protectorate_candidates)'
`
	case "powershell":
		return `Register-ArgumentCompleter -Native -CommandName protectorate -ScriptBlock {
 param($wordToComplete, $commandAst, $cursorPosition)
 $parts = @($commandAst.CommandElements | Select-Object -Skip 1 | ForEach-Object { $_.ToString() })
 if (-not $wordToComplete) { $parts += '' }
 & protectorate ` + method + ` @parts | Where-Object { $_ -notmatch '^:' } | ForEach-Object {
  $candidate, $description = $_ -split "` + "`t" + `", 2
  if (-not $description) { $description = $candidate }
  [System.Management.Automation.CompletionResult]::new($candidate, $candidate, 'ParameterValue', $description)
 }
}
`
	}
	return ""
}

var completionHelp = map[string]string{
	"bash": `Generate the autocompletion script for the bash shell.

This script depends on the 'bash-completion' package.
If it is not installed already, you can install it via your OS's package manager.

To load completions in your current shell session:

	source <(protectorate completion bash)

To load completions for every new session, execute once:

#### Linux:

	protectorate completion bash > /etc/bash_completion.d/protectorate

#### macOS:

	protectorate completion bash > $(brew --prefix)/etc/bash_completion.d/protectorate

You will need to start a new shell for this setup to take effect.`,
	"fish": `Generate the autocompletion script for the fish shell.

To load completions in your current shell session:

	protectorate completion fish | source

To load completions for every new session, execute once:

	protectorate completion fish > ~/.config/fish/completions/protectorate.fish

You will need to start a new shell for this setup to take effect.`,
	"powershell": `Generate the autocompletion script for powershell.

To load completions in your current shell session:

	protectorate completion powershell | Out-String | Invoke-Expression

To load completions for every new session, add the output of the above command
to your powershell profile.`,
	"zsh": `Generate the autocompletion script for the zsh shell.

If shell completion is not already enabled in your environment you will need
to enable it.  You can execute the following once:

	echo "autoload -U compinit; compinit" >> ~/.zshrc

To load completions in your current shell session:

	source <(protectorate completion zsh)

To load completions for every new session, execute once:

#### Linux:

	protectorate completion zsh > "${fpath[1]}/_protectorate"

#### macOS:

	protectorate completion zsh > $(brew --prefix)/share/zsh/site-functions/_protectorate

You will need to start a new shell for this setup to take effect.`,
}
