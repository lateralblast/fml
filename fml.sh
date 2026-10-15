#!/usr/bin/env bash

# Name:         fml (Fix Media Language etc)
# Version:      0.4.9
# Release:      1
# License:      CC BY-NC-SA (Creative Commons Attribution-NonCommercial-ShareAlike)
#               https://creativecommons.org/licenses/by-nc-sa/4.0/legalcode
# Group:        System
# Source:       N/A
# URL:          https://github.com/lateralblast/fml
# Distribution: UNIX
# Vendor:       UNIX
# Packager:     Richard Spindler <richard@lateralblast.com.au>
# Description:  Fix media language (default audio track) etc for media files

# Insert some shellcheck disables
# Depending on your requirements, you may want to add/remove disables
# shellcheck disable=SC2034
# shellcheck disable=SC1090
# shellcheck disable=SC2129

# Create arrays

declare -A os
declare -A script
declare -A options
declare -a file_list
declare -a actions_list
declare -a options_list
declare -a missing_tools
declare -ar required_tools=( jq file mkvinfo mkvmerge mkvpropedit mediainfo ffprobe ffmpeg )
declare -ar bool_options=( recursive verbose strict dryrun debug force yes install )

# Grab script information and put it into an associative array

script['args']="$*"
script['file']=$( realpath "$0" )
script['name']="fml"
script['path']=$( dirname "${script['file']}" )
script['modulepath']="${script['path']}/modules"
script['bin']=$( basename "${script['file']}" )

# Function: set_defaults
#
# Set defaults

set_defaults () {
  options['recursive']="false"  # option - Recursively process directory
  options['verbose']="false"    # option - Verbose mode
  options['strict']="false"     # option - Strict mode
  options['dryrun']="false"     # option - Dryrun mode
  options['debug']="false"      # option - Debug mode
  options['force']="false"      # option - Force actions
  options['yes']="false"        # option - Answer yes to questions
  options['install']="false"    # option - Install missing packages
  options['format']="JSON"      # option - Information format
  options['file']=""            # option - Input file
  options['dir']=""             # option - Input directory
  options['default']=""         # option - Default
  options['lang']=""            # option - Language
  options['track']=""           # option - Track
  options['get']=""             # option - Get
  options['set']=""             # option - Set
  options['swap']=""            # option - Swap
  options['delete']=""          # option - Delete
  options['preserve']=""        # option - Preserve
  options['filetype']=""        # option - Filetype
  os['name']=$( uname -s )
  if [[ "${os['name']}" == "Linux" ]]; then
    os['distro']=$( lsb_release -i -s 2> /dev/null )
  fi
}

set_defaults

# Function: print_message
#
# Print message

print_message () {
  local message="$1"
  local format="$2"
  local tabs
  if [[ "${format}" == "verbose" ]]; then
    echo "${message}"
  elif [[ "${format}" =~ warn ]]; then
    printf 'Warning:\t%s\n' "${message}"
  elif [[ "${options['verbose']}" == "true" ]]; then
    if [[ "${format}" =~ ing$ ]]; then
      :
    elif [[ "${format}" == "test" ]]; then
      format="${format}ing"
    elif [[ "${format}" =~ t$ ]]; then
      format="${format}ting"
    elif [[ "${format}" =~ e$ && ! "${format}" =~ otice ]]; then
      format="${format::-1}ing"
    fi
    format="${format^}"
    if [[ "${#format}" -lt 7 ]]; then
      tabs=$'\t\t'
    else
      tabs=$'\t'
    fi
    printf '%s:%s%s\n' "${format}" "${tabs}" "${message}"
  fi
}

# Function: warning_message
#
# Warning message

warning_message () {
  print_message "$1" "warn"
}

# Function: execute_message
#
# Print command

execute_message () {
  print_message "$1" "execute"
}

# Function: verbose_message
#
# Print verbose message

verbose_message () {
  print_message "$1" "verbose"
}

# Function: load_modules
#
# Load any modules found under the module path

load_modules () {
  local module
  if [[ ! -d "${script['modulepath']}" ]]; then
    return 0
  fi
  while IFS= read -r -d '' module; do
    if [[ "${script['args']}" =~ verbose ]]; then
      print_message "Module ${module}" "load"
    fi
    . "${module}"
  done < <( find "${script['modulepath']}" -name "*.sh" -print0 )
}

# Function: reset_defaults
#
# Reset defaults based on command line options

reset_defaults () {
  if [[ "${options['debug']}" == "true" ]]; then
    print_message "Enabling debug mode" "notice"
    set -x
  fi
  if [[ "${options['strict']}" == "true" ]]; then
    print_message "Enabling strict mode" "notice"
    set -u
  fi
  if [[ "${options['dryrun']}" == "true" ]]; then
    print_message "Enabling dryrun mode" "notice"
  fi
}

# Function: do_exit
#
# Selective exit (don't exit when we're running in dryrun mode)

do_exit () {
  if [[ "${options['dryrun']}" == "false" ]]; then
    exit 0
  fi
}

# Function: check_value
#
# check value (make sure that command line arguments that take values have values)

check_value () {
  local param="$1"
  local value="$2"
  if [[ "${value}" =~ ^-- ]]; then
    print_message "Value '${value}' for parameter '${param}' looks like a parameter" "verbose"
    echo ""
    if [[ "${options['force']}" == "false" ]]; then
      exit 1
    fi
  elif [[ -z "${value}" ]]; then
    print_message "No value given for parameter ${param}" "verbose"
    echo ""
    if [[ "${param}" =~ "option" ]]; then
      print_options
    elif [[ "${param}" =~ "action" ]]; then
      print_actions
    else
      print_help
    fi
    exit 0
  fi
}

# Function: get_lang_code
#
# Convert a language name to the ISO 639-2 code used in MKV metadata
# Names not listed fall back to the first 3 letters (lower case)

get_lang_code () {
  local lang_name="${1,,}"
  case "${lang_name}" in
    chinese)    echo "chi" ;;
    dutch)      echo "dut" ;;
    french)     echo "fre" ;;
    german)     echo "ger" ;;
    greek)      echo "gre" ;;
    japanese)   echo "jpn" ;;
    *)          echo "${lang_name:0:3}" ;;
  esac
}

# Function: execute_command
#
# Execute command (honours dryrun)
# The command is passed as separate arguments, so no quoting or eval is needed

execute_command () {
  local cmd_line
  if [[ "${options['verbose']}" == "true" ]]; then
    printf -v cmd_line '%q ' "$@"
    execute_message "${cmd_line% }"
  fi
  if [[ "${options['dryrun']}" != "true" ]]; then
    "$@"
  fi
}

# Function: execute_jq
#
# Execute command and filter its JSON output through jq (honours dryrun)
# Usage: execute_jq [jq option ...] filter -- command [argument ...]

execute_jq () {
  local -a jq_args=()
  local arg quoted cmd_line filter message
  while [[ $# -gt 0 && "$1" != "--" ]]; do
    jq_args+=( "$1" )
    shift
  done
  shift
  filter="${jq_args[-1]}"
  if [[ "${options['verbose']}" == "true" ]]; then
    printf -v cmd_line '%q ' "$@"
    message="jq"
    if [[ "${filter}" != "." ]]; then
      for arg in "${jq_args[@]::${#jq_args[@]}-1}"; do
        printf -v quoted '%q' "${arg}"
        message+=" ${quoted}"
      done
      message+=" '${filter}'"
    fi
    execute_message "${cmd_line}| ${message}"
  fi
  if [[ "${options['dryrun']}" != "true" ]]; then
    "$@" | jq "${jq_args[@]}"
  fi
}

# Function: print_info
#
# Print information

print_info () {
  local info="$1"
  local line param desc default
  echo ""
  echo "Usage: ${script['bin']} --action(s) [action(,action)] --option(s) [option(,option)]"
  echo ""
  if [[ "${info}" == "switch" ]]; then
    echo "${info}(es):"
  else
    echo "${info}(s):"
  fi
  echo "---------"
  while read -r line; do
    if [[ "${line}" =~ .*"# ${info}".* ]]; then
      if [[ "${info}" =~ option ]]; then
        IFS='-' read -r param desc <<< "${line}"
        IFS=']' read -r param default <<< "${param}"
        IFS='[' read -r _ param <<< "${param}"
        param="${param//\'/}"
        IFS='=' read -r _ default <<< "${default}"
        default="${default//\'/}"
        default="${default//\"/}"
        default="${default// /}"
        default="${default/\#${info}/}"
        param="${param} (default = ${default})"
      else
        IFS='#' read -r param desc <<< "${line}"
        desc="${desc/${info} -/}"
      fi
      param="${param//\"/}"
      echo "${param}"
      echo "  ${desc}"
    fi
  done < "${script['file']}"
  echo ""
}

# Function: print_help
#
# Print help/usage information

print_help () {
  print_info "switch"
}

# Function print_actions
#
# Print actions

print_actions () {
  print_info "action"
}

# Function: print_options
#
# Print options

print_options () {
  print_info "option"
}

# Function: print_usage
#
# Print Usage

print_usage () {
  case "$1" in
    all|full)
      print_help
      print_actions
      print_options
      ;;
    help)
      print_help
      ;;
    action*)
      print_actions
      ;;
    option*)
      print_options
      ;;
    *)
      print_help
      ;;
  esac
}

# Function: print_version
#
# Print version information

print_version () {
  script['version']=$( awk '/^# Version/ {print $3; exit}' "${script['file']}" )
  echo "${script['version']}"
}

# Function: check_shellcheck
#
# Run Shellcheck

check_shellcheck () {
  if command -v shellcheck > /dev/null; then
    shellcheck "${script['file']}"
  fi
}

# Function: is_bool_option
#
# Check a name is an option that can be switched on or off with --option

is_bool_option () {
  [[ " ${bool_options[*]} " == *" $1 "* ]]
}

# Function: process_options
#
# Handle options (a "no" prefix switches the option off)

process_options () {
  local option="$1"
  local value="true"
  if ! is_bool_option "${option}" && [[ "${option}" =~ ^no ]]; then
    option="${option:2}"
    value="false"
  fi
  if ! is_bool_option "${option}"; then
    warning_message "Unknown option \"$1\" (see --usage options)"
    exit 1
  fi
  options["${option}"]="${value}"
  print_message "${option} to ${value}" "set"
}

# Function: print_environment
#
# Print environment

print_environment () {
  local option
  echo "Environment (Options):"
  while IFS= read -r option; do
    printf 'Option %s\tis set to %s\n' "${option}" "${options[${option}]}"
  done < <( printf '%s\n' "${!options[@]}" | sort )
}

# Function: print_defaults
#
# Print defaults

print_defaults () {
  local default
  echo "Defaults:"
  while IFS= read -r default; do
    printf 'Default %s\tis set to %s\n' "${default}" "${options[${default}]}"
  done < <( printf '%s\n' "${!options[@]}" | sort )
}

# Function: install_package
#
# Install package

install_package () {
  local package="$1"
  case "${os['name']}" in
    "Linux")
      if [[ "${os['distro']}" == "Ubuntu" ]]; then
        execute_command sudo apt install -y "${package}"
      else
        warning_message "Don't know how to install \"${package}\" on this distribution"
      fi
      ;;
    "Darwin")
      execute_command brew install "${package}"
      ;;
    *)
      warning_message "Don't know how to install \"${package}\" on this OS"
      ;;
  esac
}

# Function: get_package
#
# Get the name of the package that provides a command

get_package () {
  case "$1" in
    *mkv*)
      echo "mkvtoolnix"
      ;;
    ff*)
      echo "ffmpeg"
      ;;
    *)
      echo "$1"
      ;;
  esac
}

# Function: check_package
#
# Install the package that provides a command

check_package () {
  install_package "$( get_package "$1" )"
}

# Function: report_tools
#
# Print found/missing status for each required command and fill missing_tools

report_tools () {
  local tool tool_path
  missing_tools=()
  for tool in "${required_tools[@]}"; do
    tool_path=$( command -v "${tool}" )
    if [[ -n "${tool_path}" ]]; then
      printf 'Found:\t\t%s (%s)\n' "${tool}" "${tool_path}"
    else
      printf 'Missing:\t%s\n' "${tool}"
      missing_tools+=( "${tool}" )
    fi
  done
}

# Function: check_tools
#
# Report which required commands are installed, installing missing packages
# if the install option is set (exit 1 if any are still missing)

check_tools () {
  local tool package
  local -A installed=()
  report_tools
  if [[ "${#missing_tools[@]}" -gt 0 && "${options['install']}" == "true" ]]; then
    for tool in "${missing_tools[@]}"; do
      package=$( get_package "${tool}" )
      if [[ -z "${installed[${package}]:-}" ]]; then
        install_package "${package}"
        installed["${package}"]="true"
      fi
    done
    if [[ "${options['dryrun']}" == "true" ]]; then
      return 0
    fi
    echo "Rechecking:"
    report_tools
  fi
  if [[ "${#missing_tools[@]}" -gt 0 ]]; then
    if [[ "${options['install']}" != "true" ]]; then
      echo "Use --option install to install missing packages"
    fi
    exit 1
  fi
}

# Function: check_environment
#
# Check environment

check_environment () {
  local tool
  for tool in "${required_tools[@]}"; do
    if ! command -v "${tool}" > /dev/null; then
      warning_message "Command \"${tool}\" not found"
      if [[ "${options['install']}" == "true" ]]; then
        check_package "${tool}"
      else
        exit 1
      fi
    fi
  done
}

# Function: check_file
#
# Check file exists and get type

check_file () {
  if [[ -z "${options['file']}" ]]; then
    warning_message "No file specified"
    exit 1
  elif [[ ! -f "${options['file']}" ]]; then
    warning_message "File \"${options['file']}\" does not exist"
    exit 1
  fi
  options['filetype']=$( file "${options['file']}" )
}

# Function: get_file_list
#
# Get file list from directory

get_file_list () {
  local -a find_args=()
  if [[ ! -d "${options['dir']}" ]]; then
    warning_message "Directory \"${options['dir']}\" does not exist"
    exit 1
  fi
  if [[ "${options['recursive']}" != "true" ]]; then
    find_args=( -maxdepth 1 )
  fi
  mapfile -d '' -t file_list < <( find "${options['dir']}" "${find_args[@]}" -type f -print0 | sort -z )
}

# Function: for_each_file
#
# Run a function on the file, or on every file in the directory if one is given

for_each_file () {
  local callback="$1"
  local file
  if [[ -z "${options['dir']}" ]]; then
    "${callback}"
  else
    get_file_list
    for file in "${file_list[@]}"; do
      options['file']="${file}"
      "${callback}"
    done
  fi
}

# Function: get_audio_tracks
#
# Print the audio tracks of an MKV file as a compact JSON array

get_audio_tracks () {
  mkvmerge -F json -i "$1" | jq -c '[.tracks[] | select(.type == "audio")]'
}

# Function: set_default_audio
#
# Make the first audio track in a language the default, and clear the default flag
# on all the other audio tracks

set_default_audio () {
  local media_file="$1"
  local lang="$2"
  local tracks lang_track track_no
  local -a other_tracks=()
  local -a sub_command=()
  tracks=$( get_audio_tracks "${media_file}" )
  lang_track=$( jq -r --arg lang "${lang}" '[to_entries[] | select(.value.properties.language == $lang) | .key + 1] | .[0] // empty' <<< "${tracks}" )
  mapfile -t other_tracks < <( jq -r --arg lang "${lang}" 'to_entries[] | select(.value.properties.language != $lang) | .key + 1' <<< "${tracks}" )
  for track_no in "${other_tracks[@]}"; do
    sub_command+=( --edit "track:a${track_no}" --set flag-default=0 )
  done
  execute_command mkvpropedit "${media_file}" "${sub_command[@]}" --edit "track:a${lang_track}" --set flag-default=1
}

# Function: replace_file
#
# Replace a file with its rewritten temporary copy

replace_file () {
  local temp_file="$1"
  local media_file="$2"
  if [[ "${options['dryrun']}" == "true" || -f "${temp_file}" ]]; then
    execute_command mv -f -- "${temp_file}" "${media_file}"
  else
    warning_message "Failed to process file \"${media_file}\""
  fi
}

# Function: set_file_info
#
# Set file information

set_file_info () {
  local media_file set_lang tracks
  check_file
  check_environment
  media_file="${options['file']}"
  if [[ ! "${options['filetype']}" =~ Matroska ]]; then
    return 0
  fi
  if [[ -z "${options['default']}" ]]; then
    if [[ -n "${options['lang']}" ]]; then
      set_lang=$( get_lang_code "${options['lang']}" )
      execute_command mkvpropedit "${media_file}" --edit track:a1 --set "language=${set_lang}"
    fi
    return 0
  fi
  if [[ "${options['set']}" != "lang" ]]; then
    return 0
  fi
  set_lang=$( get_lang_code "${options['default']}" )
  tracks=$( get_audio_tracks "${media_file}" )
  if ! jq -e --arg lang "${set_lang}" 'any(.[]; .properties.language == $lang)' <<< "${tracks}" > /dev/null; then
    warning_message "File \"${media_file}\" does not have a track with language \"${options['default']}\""
  elif jq -e --arg lang "${set_lang}" 'any(.[]; .properties.language == $lang and .properties.default_track == false)' <<< "${tracks}" > /dev/null; then
    set_default_audio "${media_file}" "${set_lang}"
  else
    verbose_message "Default language for file \"${media_file}\" is already ${options['default']}"
    verbose_message "Checking for non \"${options['default']}\" tracks that are set to default"
    if jq -e --arg lang "${set_lang}" 'any(.[]; .properties.language != $lang and .properties.default_track == true)' <<< "${tracks}" > /dev/null; then
      set_default_audio "${media_file}" "${set_lang}"
    fi
  fi
}

# Function: set_info
#
# Set information for file(s)

set_info () {
  for_each_file set_file_info
}

# Function: swap_file_info
#
# Swap information for file

swap_file_info () {
  local media_file temp_file
  check_file
  check_environment
  media_file="${options['file']}"
  if [[ "${options['swap']}" != "lang" ]]; then
    return 0
  fi
  if [[ "${options['filetype']}" =~ Matroska ]]; then
    execute_command mkvpropedit "${media_file}" --edit track:a1 --set flag-default=0 --edit track:a2 --set flag-default=1
  elif [[ "${options['filetype']}" =~ AVI ]]; then
    temp_file="${media_file%.*}-temp.avi"
    execute_command ffmpeg -i "${media_file}" -map 0:v? -map 0:a:1 -map 0:a:0 -map 0:a:2? -map 0:s? -disposition:a:0 default -disposition:a:1 none -c copy "${temp_file}"
    replace_file "${temp_file}" "${media_file}"
  fi
}

# Function: swap_info
#
# Swap information for file(s)

swap_info () {
  for_each_file swap_file_info
}

# Function: get_file_info
#
# Get file information

get_file_info () {
  local media_file lang format
  check_file
  check_environment
  media_file="${options['file']}"
  if [[ "${options['filetype']}" =~ Matroska ]]; then
    if [[ "${options['get']}" == "lang" ]]; then
      if [[ "${options['lang']}" == "default" ]]; then
        execute_jq '.tracks[] | select(.type == "audio") | select (.properties.default_track == true) | .properties.language' -- mkvmerge -F json -i "${media_file}"
      elif [[ -z "${options['lang']}" ]]; then
        execute_jq '.tracks[] | select(.type == "audio") | select (.id == 1) | { language: .properties.language } | .language' -- mkvmerge -F json -i "${media_file}"
      else
        lang=$( get_lang_code "${options['lang']}" )
        if get_audio_tracks "${media_file}" | jq -e --arg lang "${lang}" 'any(.[]; .properties.language == $lang)' > /dev/null; then
          # shellcheck disable=SC2016
          execute_jq --arg lang "${lang}" '.tracks[] | select(.type == "audio") | select (.properties.language == $lang)' -- mkvmerge -F json -i "${media_file}"
        else
          warning_message "File \"${media_file}\" does not have a track with language \"${options['lang']}\""
        fi
      fi
    elif [[ "${options['format']}" == "JSON" ]]; then
      execute_jq . -- mkvmerge -F json -i "${media_file}"
    else
      execute_command mkvmerge -i "${media_file}"
    fi
  elif [[ -z "${options['format']}" || "${options['format']}" == "TEXT" ]]; then
    execute_command mediainfo "${media_file}"
  else
    format="${options['format']^^}"
    if [[ "${options['get']}" == "lang" ]]; then
      execute_jq '.media.track[] | select (."@type"=="Audio") | select (."@typeorder"=="1") | .Language' -- mediainfo "--Output=${format}" "${media_file}"
    elif [[ "${options['format']}" == "JSON" ]]; then
      execute_jq . -- mediainfo "--Output=${format}" "${media_file}"
    else
      execute_command mediainfo "--Output=${format}" "${media_file}"
    fi
  fi
}

# Function: get_info
#
# Get information from file(s)

get_info () {
  for_each_file get_file_info
}

# Function: remove_audio_tracks
#
# Remove audio tracks from an MKV file one at a time, reading the track list again
# after each removal as the track IDs change
# Mode "match" removes the tracks in the language, mode "other" removes all the rest

remove_audio_tracks () {
  local media_file="$1"
  local lang="$2"
  local mode="$3"
  local operator="=="
  local filter no_tracks counter track_no temp_file
  if [[ "${mode}" == "other" ]]; then
    operator="!="
  fi
  filter=".[] | select(.properties.language ${operator} \$lang) | .id"
  no_tracks=$( get_audio_tracks "${media_file}" | jq -r --arg lang "${lang}" "${filter}" | wc -l )
  for (( counter = 0; counter < no_tracks; counter++ )); do
    track_no=$( get_audio_tracks "${media_file}" | jq -r --arg lang "${lang}" "${filter}" | head -1 )
    if [[ -n "${track_no}" ]]; then
      temp_file="${media_file}-${track_no}"
      execute_command mkvmerge -o "${temp_file}" --audio-tracks "!${track_no}" "${media_file}"
      replace_file "${temp_file}" "${media_file}"
    fi
  done
}

# Function: delete_file_info
#
# Delete file information

delete_file_info () {
  local media_file set_lang track_no temp_file
  check_file
  check_environment
  media_file="${options['file']}"
  if [[ "${options['filetype']}" =~ Matroska ]]; then
    if [[ "${options['delete']}" == "lang" ]]; then
      set_lang=$( get_lang_code "${options['lang']:-${options['default']}}" )
      if [[ -z "${set_lang}" ]]; then
        warning_message "No language specified (use --lang)"
        exit 1
      fi
      remove_audio_tracks "${media_file}" "${set_lang}" "match"
    fi
  elif [[ "${options['filetype']}" =~ AVI ]]; then
    if [[ "${options['delete']}" == "track" ]]; then
      temp_file="${media_file%.*}-temp.avi"
      if [[ "${options['track']}" == "last" ]]; then
        track_no=$( ffprobe "${media_file}" 2>&1 | grep -c "Audio:" )
      else
        track_no="${options['track']}"
      fi
      if [[ ! "${track_no}" =~ ^[0-9]+$ || "${track_no}" -lt 1 ]]; then
        warning_message "Invalid audio track \"${track_no}\" (use a track number or \"last\")"
        exit 1
      fi
      execute_command ffmpeg -i "${media_file}" -map 0 -map "-0:a:$(( track_no - 1 ))" -c copy "${temp_file}"
      replace_file "${temp_file}" "${media_file}"
    fi
  fi
}

# Function: delete_info
#
# Delete information from file(s)

delete_info () {
  for_each_file delete_file_info
}

# Function: preserve_file_info
#
# Preserve file information

preserve_file_info () {
  local media_file set_lang
  check_file
  check_environment
  media_file="${options['file']}"
  if [[ "${options['filetype']}" =~ Matroska && "${options['preserve']}" == "lang" ]]; then
    set_lang=$( get_lang_code "${options['lang']:-${options['default']}}" )
    if [[ -z "${set_lang}" ]]; then
      warning_message "No language specified (use --lang)"
      exit 1
    fi
    remove_audio_tracks "${media_file}" "${set_lang}" "other"
  fi
}

# Function: preserve_info
#
# Preserve information from file(s)

preserve_info () {
  for_each_file preserve_file_info
}

# Function: process_actions
#
# Handle actions

process_actions () {
  local actions="$1"
  case "${actions}" in
    check)                # action - Check required packages are installed
      check_tools
      do_exit
      ;;
    get|info)             # action - Get file information
      get_info
      ;;
    help)                 # action - Print actions help
      print_actions
      do_exit
      ;;
    delete*)              # action - Delete file information
      delete_info
      ;;
    version)              # action - Print version
      print_version
      do_exit
      ;;
    pres*|leave*)         # action - Preserve/leave file information
      preserve_info
      ;;
    printenv*)            # action - Print environment
      print_environment
      do_exit
      ;;
    printdefaults)        # action - Print defaults
      print_defaults
      do_exit
      ;;
    set)                  # action - Set file information
      set_info
      ;;
    shellcheck)           # action - Shellcheck script
      check_shellcheck
      do_exit
      ;;
    swap)                 # action - Swap file information
      swap_info
      ;;
    *)
      print_actions
      do_exit
      ;;
  esac
}

# Function: parse_args
#
# Handle command line arguments

parse_args () {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --action*|"-a")             # switch - Action to perform
        check_value "$1" "$2"
        actions_list+=("$2")
        shift 2
        ;;
      --check|"-C")               # switch - Check required packages are installed
        actions_list+=("check")
        shift
        ;;
      --debug|"-Z")               # switch - Enable debug mode
        options['debug']="true"
        shift
        ;;
      --default*|"-D")            # switch - Set default
        check_value "$1" "$2"
        options['default']="$2"
        shift 2
        ;;
      --delete|"-z")              # switch - Delete item from file (e.g. track)
        check_value "$1" "$2"
        options['delete']="$2"
        actions_list+=("delete")
        shift 2
        ;;
      --dir*|"-d")                # switch - Directory to process
        check_value "$1" "$2"
        options['dir']="$2"
        shift 2
        ;;
      --dryrun|"-T")              # switch - Enable dryrun mode
        options['dryrun']="true"
        shift
        ;;
      --file|"-i")                # switch - File to process
        check_value "$1" "$2"
        options['file']="$2"
        shift 2
        ;;
      --force|"-f")               # switch - Enable force mode
        options['force']="true"
        shift
        ;;
      --format*|"-F")             # switch - Set output format
        check_value "$1" "$2"
        options['format']="${2^^}"
        shift 2
        ;;
      --get|"-g")                 # switch - Get information about file
        check_value "$1" "$2"
        options['get']="$2"
        actions_list+=("get")
        shift 2
        ;;
      --help|"-h")                # switch - Print help information
        print_help
        exit 0
        ;;
      --info|"-I")                # switch - Get information about file
        actions_list+=("info")
        shift
        ;;
      --lang*|"-l")               # switch - Set language
        check_value "$1" "$2"
        options['lang']="$2"
        shift 2
        ;;
      --opt*|"-o")                # switch - Options to set
        check_value "$1" "$2"
        options_list+=("$2")
        shift 2
        ;;
      --preserve*|--leave*|"-p")  # switch - Preserve item from file (e.g. track)
        check_value "$1" "$2"
        options['preserve']="$2"
        actions_list+=("preserve")
        shift 2
        ;;
      --recursive|"-r")           # switch - Enable recursive mode
        options['recursive']="true"
        shift
        ;;
      --set|"-c")                 # switch - Set information about file
        check_value "$1" "$2"
        options['set']="$2"
        actions_list+=("set")
        shift 2
        ;;
      --shellcheck|"-S")          # switch - Run shellcheck against script
        actions_list+=("shellcheck")
        shift
        ;;
      --strict|"-m")              # switch - Enable strict mode
        options['strict']="true"
        shift
        ;;
      --swap|"-s")                # switch - Swap information about file
        check_value "$1" "$2"
        options['swap']="$2"
        actions_list+=("swap")
        shift 2
        ;;
      --track|"-t")               # switch - Track to perform operation on
        check_value "$1" "$2"
        options['track']="$2"
        shift 2
        ;;
      --usage|"-U")               # switch - Display usage
        check_value "$1" "$2"
        print_usage "$2"
        exit 0
        ;;
      --verbose|"-v")             # switch - Enable verbose mode
        options['verbose']="true"
        shift
        ;;
      --version|"-V")             # switch - Print version information
        print_version
        exit 0
        ;;
      *)
        print_help
        exit 1
        ;;
    esac
  done
}

# Function: process_list
#
# Run a handler on each item of a list of (optionally comma separated) entries
# Usage: process_list handler entry [entry ...]

process_list () {
  local handler="$1"
  local entry item
  local -a items
  shift
  for entry in "$@"; do
    IFS=',' read -r -a items <<< "${entry}"
    for item in "${items[@]}"; do
      "${handler}" "${item}"
    done
  done
}

# Function: main
#
# Main routine

main () {
  load_modules
  # Do some early command line argument processing
  if [[ -z "${script['args']}" ]]; then
    print_help
    exit 0
  fi
  parse_args "$@"
  # Process options
  process_list process_options "${options_list[@]}"
  # Reset defaults based on switches
  reset_defaults
  # Process actions
  process_list process_actions "${actions_list[@]}"
}

main "$@"
