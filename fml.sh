#!/usr/bin/env bash

# Name:         fml (Fix Media Language etc)
# Version:      0.1.8
# Release:      1
# License:      CC-BA (Creative Commons By Attribution)
#               http://creativecommons.org/licenses/by/4.0/legalcode
# Group:        System
# Source:       N/A
# URL:          https://github.com/lateralblast/just
# Distribution: UNIX
# Vendor:       UNIX
# Packager:     Richard Spindler <richard@lateralblast.com.au>
# Description:  A template for writing shell scripts

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

# Grab script information and put it into an associative array

script['args']="$*"
script['file']="$0"
script['name']="just"
script['file']=$( realpath "${script['file']}" )
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
  options['format']="JSON"      # option - Information format
  os['name']=$( uname -s )
  if [ "${os['name']}" = "Linux" ]; then
    os['distro']=$( lsb_release -i -s 2> /dev/null )
  fi
}

set_defaults

# Function: print_message
#
# Print message

print_message () {
  message="$1"
  format="$2"
  if [ "${format}" = "verbose" ]; then
    echo "${message}"
  else
    if [[ "${format}" =~ warn ]]; then
      echo -e "Warning:\t${message}"
    else
      if [ "${options['verbose']}" = "true" ]; then
        if [[ "${format}" =~ ing$ ]]; then
          format="${format^}"
        else
          if [[ "${format}" =~ t$ ]]; then
            if [ "${format}" = "test" ]; then
              format="${format}ing"
            else
              format="${format^}ting"
            fi
          else
            if [[ "${format}" =~ e$ ]]; then
              if [[ ! "${format}" =~ otice ]]; then
                format="${format::-1}"
                format="${format^}ing"
              fi
            fi
          fi
        fi 
        format="${format^}"
        length="${#format}"
        if [ "${length}" -lt 7 ]; then
          tabs="\t\t"
        else
          tabs="\t"
        fi
        echo -e "${format}:${tabs}${message}"
      fi
    fi
  fi
}

# Function: warning_message
#
# Warning message

warning_message () {
  message="$1"
  print_message "${message}" "warn"
}

# Function: execute_message
#
# Print command

execute_message () {
  message="$1"
  print_message "${message}" "execute"
}

# Function: verbose_message
#
# Print verbose message

verbose_message () {
  message="$1"
  print_message "${message}" "verbose"
}

# Load modules

if [ -d "${script['modulepath']}" ]; then
  modules=$( find "${script['modulepath']}" -name "*.sh" )
  for module in ${modules}; do
    if [[ "${script['args']}" =~ "verbose" ]]; then
     print_message "Module ${module}" "load"
    fi
    . "${module}"
  done
fi

# Function: reset_defaults
#
# Reset defaults based on command line options

reset_defaults () {
  if [ "${options['debug']}" = "true" ]; then
    print_message "Enabling debug mode" "notice"
    set -x
  fi
  if [ "${options['strict']}" = "true" ]; then
    print_message "Enabling strict mode" "notice"
    set -u
  fi
  if [ "${options['dryrun']}" = "true" ]; then
    print_message "Enabling dryrun mode" "notice"
  fi
}

# Function: do_exit
#
# Selective exit (don't exit when we're running in dryrun mode)

do_exit () {
  if [ "${options['dryrun']}" = "false" ]; then
    exit
  fi
}

# Function: check_value
#
# check value (make sure that command line arguments that take values have values)

check_value () {
  param="$1"
  value="$2"
  if [[ "${value}" =~ "--" ]]; then
    print_message "Value '$value' for parameter '$param' looks like a parameter" "verbose"
    echo ""
    if [ "${options['force']}" = "false" ]; then
      do_exit
    fi
  else
    if [ "${value}" = "" ]; then
      print_message "No value given for parameter $param" "verbose"
      echo ""
      if [[ "${param}" =~ "option" ]]; then
        print_options
      else
        if [[ "${param}" =~ "action" ]]; then
          print_actions
        else
          print_help
        fi
      fi
      exit
    fi
  fi
}

# Function: execute_command
#
# Execute command

execute_command () {
  command="$1"
  privilege="$2"
  if [[ "${privilege}" =~ su ]]; then
    command="sudo sh -c \"${command}\""
  fi
  if [ "${options['verbose']}" = "true" ]; then
    execute_message "${command}"
  fi
  if [ "${options['dryrun']}" = "false" ]; then
    eval "${command}"
  fi
}

# Function: print_info
#
# Print information

print_info () {
  info="$1"
  echo ""
  echo "Usage: ${script['bin']} --action(s) [action(,action)] --option(s) [option(,option)]"
  echo ""
  if [ "${info}" = "switch" ]; then
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
      echo "${param}"
      echo "  ${desc}"
    fi
  done < "${script['file']}"
  echo ""
}

# Function: print_help
#
# Print help/usage insformation

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
  usage="$1"
  case $usage in
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
      shift
      ;;
  esac
}

# Function: print_version
#
# Print version information

print_version () {
  script['version']=$( grep '^# Version' < "$0" | awk '{print $3}' )
  echo "${script['version']}"
}

# Function: check_shellcheck
#
# Run Shellcheck

check_shellcheck () {
  bin_test=$( command -v shellcheck | grep -c shellcheck )
  if [ ! "$bin_test" = "0" ]; then
    shellcheck "${script['file']}"
  fi
}

# Do some early command line argument processing

if [ "${script['args']}" = "" ]; then
  print_help
  exit
fi

# Function: process_options
#
# Handle options

process_options () {
  option="$1"
  if [[ "${option}" =~ ^no ]]; then
    option="${option:2}"
    value="false"
  else
    value="true"
  fi
  options["${option}"]="true"
  print_message "${option} to ${value}" "set"
}

# Function: print_environment
#
# Print environment

print_environment () {
  echo "Environment (Options):"
  for option in "${!options[@]}"; do
    value="${options[${option}]}"
    echo -e "Option ${option}\tis set to ${value}"
  done
}

# Function: print_defaults
#
# Print defaults

print_defaults () {
  echo "Defaults:"
  for default in "${!options[@]}"; do
    value="${options[${default}]}"
    echo -e "Default ${default}\tis set to ${value}"
  done
}

# Function: install_package
#
# Install package

install_package () {
  package="$1"
  case ${os['name']} in
    "Linux")
      if [ "${os['distro']}" = "Ubuntu" ]; then
        execute_message "apt install -y ${package}" "linuxsu"
      fi
      ;;
    "Darwin")
      execute_message "brew install ${package}"
      ;;
  esac

}

# Function: check_package
#
# Check package

check_package () {
  package="$1"
  case ${package} in
    *mkv*)
      install_package "mkvtoolnix"
      ;;
    *)
      install_package "${package}"
      ;;
  esac
}

# Function: check_environment
#
# Check environment

check_environment () {
  for test_file in jq mkvinfo mediainfo; do
    bin_test=$( command -v "${test_file}" | grep -c "${test_file}" )
    if [ "$bin_test" = "0" ]; then
      warning_message "Command \"${test_file}\" not found"
      if [ "${options['install']}" = "true" ]; then
        check_package "${test_file}"
      else
        do_exit
      fi
    fi
  done
}

# Function: check_file
#
# Check file exists and get type

check_file () {
  if [ "${options['file']}" = "" ]; then
    warning_message "No file specified"
    do_exit
  else
    if [ ! -f "${options['file']}" ]; then
      warning_message "File \"${options['file']}\" does not exist"
      do_exit
    else
      options['filetype']=$( file "${options['file']}" )
    fi
  fi
}

# Function: get_file_list
#
# Get file list from directory

get_file_list () {
  if [ -d "${options['dir']}" ]; then
    if [ "${options['recursive']}" = "true" ]; then
      while IFS= read -r -u3 -d $'\0' file; do
        file_list+=( "$file" )
      done 3< <( find "${options['dir']}" -type f -print0 )
    else
      while IFS= read -r -u3 -d $'\0' file; do
        file_list+=( "$file" )
      done 3< <( find "${options['dir']}" -type f -maxdepth 1 -print0 )
    fi
  else
    warning_message "Directory \"${options['dir']}\" does not exist"
    do_exit
  fi
}

# Function: set_file_info
#
# Set file information

set_file_info () {
  check_file 
  check_environment
  if [ "${options['default']}" = "" ]; then
    if [[ "${options['filetype']}" =~ Matroska ]]; then
      if [ ! "${options['lang']}" = "" ]; then
        set_lang=$( echo "${options['lang']}" | tr '[:upper:]' '[:lower:]' | cut -c1-3 )
        execute_command "mkvpropedit \"${options['file']}\" --edit track:a1 --set language=${set_lang}"
      fi
    fi
  else
    if [[ "${options['filetype']}" =~ Matroska ]]; then
      if [ "${options['set']}" = "lang" ]; then
        set_lang=$( echo "${options['default']}" | tr '[:upper:]' '[:lower:]' | cut -c1-3 )
        get_value=$( mkvmerge -F json -i "${options['file']}" | jq ".tracks[] | select(.type == \"audio\") | select (.properties.language == \"${set_lang}\") | .properties.default_track" )
        if [ "${get_value}" = "" ]; then
          warning_message "File \"${options['file']}\" does not have a track with language \"${options['default']}\""
        else
          if [ "${get_value}" = "false" ]; then
            lang_track=$( mkvmerge -F json -i "${options['file']}" | jq ".tracks[] | select(.type == \"audio\") | select (.properties.language == \"${set_lang}\") | .id" )
            other_track=$( mkvmerge -F json -i "${options['file']}" | jq ".tracks[] | select(.type == \"audio\") | select (.properties.language != \"${set_lang}\") | .id" )
            sub_command=""
            IFS=$'\n' read -r -a track_nos <<< "${other_track[*]}"
            for track_no in "${track_nos[@]}"; do
              if [ "${sub_command}" = "" ]; then
                sub_command="--edit track:a${track_no} --set flag-default=0"
              else
                sub_command="${sub_command} --edit track:a${track_no} --set flag-default=0"
              fi
            done
            execute_command "mkvpropedit \"${options['file']}\" ${sub_command} --edit track:a${lang_track} --set flag-default=1" 
          else
            verbose_message "Default language for file \"${options['file']}\" is already ${options['default']}"
          fi
        fi
      fi
    fi
  fi
}

# Function: set_info
#
# Set information for file(s)

set_info () {
  if [ "${options['dir']}" = "" ]; then
    set_file_info
  else
    get_file_list
    for file in "${file_list[@]}"; do
      options['file']="${file}"
      set_file_info
    done
  fi
}

# Function: swap_file_info
#
# Swap information for file

swap_file_info () {
  check_file 
  check_environment
  if [ "${options['swap']}" = "lang" ]; then
    if [[ "${options['filetype']}" =~ Matroska ]]; then
      execute_command "mkvpropedit \"${options['file']}\" --edit track:a1 --set flag-default=0 --edit track:a2 --set flag-default=1"
    fi
  fi
}

# Function: swap_info
#
# Swap information for file(s)

swap_info () {
  if [ "${options['dir']}" = "" ]; then
    swap_file_info
  else
    get_file_list
    for file in "${file_list[@]}"; do
      options['file']="${file}"
      swap_file_info
    done
  fi
}

# Function: get_file_info
#
# Get file information

get_file_info () {
  check_file 
  check_environment
  if [[ "${options['filetype']}" =~ Matroska ]]; then
    if [ "${options['get']}" = "lang" ]; then
      if [ "${options['lang']}" = "default" ]; then
        execute_command "mkvmerge -F json -i \"${options['file']}\" | jq '.tracks[] | select(.type == \"audio\") | select (.properties.default_track == true) | .properties.language'"
      else
        if [ "${options['lang']}" = "" ]; then
          execute_command "mkvmerge -F json -i \"${options['file']}\" | jq '.tracks[] | select(.type == \"audio\") | select (.id == 1) | { language: .properties.language } | .language'"
        else
          lang=$( echo "${options['lang']}" | tr '[:upper:]' '[:lower:]' | cut -c1-3 )
          get_value=$( mkvmerge -F json -i "${options['file']}" | jq -r ".tracks[] | select(.type == \"audio\") | select (.properties.language == \"${lang}\") | .properties.language" )
          if [ ! "${get_value}" = "${lang}" ]; then
            warning_message "File \"${options['file']}\" does not have a track with language \"${options['lang']}\""
          else
            execute_command "mkvmerge -F json -i \"${options['file']}\" | jq '.tracks[] | select(.type == \"audio\") | select (.properties.language == \"${lang}\")'"
          fi
        fi
      fi
    else
      format=$( echo "${options['format']}" | tr '[:upper:]' '[:lower:]' )
      if [ "${options['format']}" = "json" ]; then
        execute_command "mkvmerge -F ${format} -i \"${option['file']}j\" | jq"
      fi
    fi
  else
    if [ "${options['format']}" = "" ]; then
      execute_command "mediainfo ${options['file']}"
    else
      format=$( echo "${options['format']}" | tr '[:lower:]' '[:upper:]' )
      if [ "${options['get']}" = "lang" ]; then
          execute_command "mediainfo --Output=${format} \"${options['file']}\" | jq '.media.track[] | select (.\"@type\"==\"Audio\") | select (.\"@typeorder\"==\"1\")' |jq '.Language'"
      else
        if [ "${options['format']}" = "JSON" ]; then
          execute_command "mediainfo --Output=${format} \"${options['file']}\" | jq"
        else
          execute_command "mediainfo --Output=${format} \"${options['file']}\""
        fi
      fi
    fi
  fi
}

# Function: get_info
#
# Get information from file(s)

get_info () {
  if [ "${options['dir']}" = "" ]; then
    get_file_info
  else
    get_file_list
    echo "${file_list[@]}"
    for file in "${file_list[@]}"; do
      options['file']="${file}"
      get_file_info
    done
  fi
}

# Function: delete_file_info
#
# Delete file information

delete_file_info () {
  check_file 
  check_environment
  if [[ "${options['filetype']}" =~ Matroska ]]; then
    if [ "${options['delete']}" = "lang" ]; then
      set_lang=$( echo "${options['default']}" | tr '[:upper:]' '[:lower:]' | cut -c1-3 )
      other_track=$( mkvmerge -F json -i "${options['file']}" | jq ".tracks[] | select(.type == \"audio\") | select (.properties.language == \"${set_lang}\") | .id" )
      IFS=$'\n' read -r -a track_nos <<< "${other_track[*]}"
      for track_no in "${track_nos[@]}"; do
        temp_file="${options['file']}-${track_no}"
        execute_command "mkvmerge -o \"${temp_file}\" --audio-tracks \!${track_no} ${options['file']}"
        if [ -f "${test_file}" ]; then
          execute_command "rm \"${options['file']}\""
          execute_command "mv \"${temp_file}\" \"${options['file']}\""
        else
          warning_message "Failed to process file \"${options['file']}\""
        fi
      done
    fi
  fi
}

# Function: delete_info
#
# Delete information from file(s)

delete_info () {
  if [ "${options['dir']}" = "" ]; then
    delete_file_info
  else
    get_file_list
    for file in "${file_list[@]}"; do
      options['file']="${file}"
      delete_file_info
    done
  fi
}

# Function: preserve_file_info
#
# Preserve file information

preserve_file_info () {
  check_file 
  check_environment
  if [[ "${options['filetype']}" =~ Matroska ]]; then
    if [ "${options['preserve']}" = "lang" ]; then
      set_lang=$( echo "${options['default']}" | tr '[:upper:]' '[:lower:]' | cut -c1-3 )
      other_track=$( mkvmerge -F json -i "${options['file']}" | jq ".tracks[] | select(.type == \"audio\") | select (.properties.language != \"${set_lang}\") | .id" )
      IFS=$'\n' read -r -a track_nos <<< "${other_track[*]}"
      for track_no in "${track_nos[@]}"; do
        temp_file="${options['file']}-${track_no}"
        execute_command "mkvmerge -o \"${temp_file}\" --audio-tracks \!${track_no} ${options['file']}"
        if [ -f "${test_file}" ]; then
          execute_command "rm \"${options['file']}\""
          execute_command "mv \"${temp_file}\" \"${options['file']}\""
        else
          warning_message "Failed to process file \"${options['file']}\""
        fi
      done
    fi
  fi
}

# Function: preserve_info
#
# Preserve information from file(s)

preserve_info () {
  if [ "${options['dir']}" = "" ]; then
    preserve_file_info
  else
    get_file_list
    for file in "${file_list[@]}"; do
      options['file']="${file}"
      preserve_file_info
    done
  fi
}

# Function: process_actions
#
# Handle actions

process_actions () {
  actions="$1"
  case ${actions} in
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

# Handle command line arguments

while test $# -gt 0; do
  case $1 in
    --action*)            # switch - Action to perform
      check_value "$1" "$2"
      actions_list+=("$2")
      shift 2
      ;;
    --debug)              # switch - Enable debug mode
      options['debug']="true"
      shift
      ;;
    --default*)           # switch - Set default
      check_value "$1" "$2"
      options['default']="$2"
      shift 2
      ;;
    --delete)             # switch - Delete item from file (e.g. track)
      check_value "$1" "$2"
      options['delete']="$2"
      actions_list+=("delete")
      shift 2
      ;;
    --dir*)               # switch - Directory to process
      check_value "$1" "$2"
      options['dir']="$2"
      shift 2
      ;;
    --dryrun)             # switch - Enable dryrun mode
      options['dryrun']="true"
      shift
      ;;
    --file)               # switch - File to process
      check_value "$1" "$2"
      options['file']="$2"
      shift 2
      ;;
    --force)              # switch - Enable force mode
      options['force']="true"
      shift
      ;;
    --format*)            # switch - Set output format
      check_value "$1" "$2"
      options['format']="$2"
      shift 2
      ;;
    --get)                # switch - Get information about file
      check_value "$1" "$2"
      options['get']="$2"
      actions_list+=("get")
      shift 2
      ;;
    --help|-h)            # switch - Print help information
      print_help
      shift
      exit
      ;;
    --info)               # switch - Get information about file
      actions_list+=("info")
      shift
      ;;
    --lang*)              # switch - Set language
      check_value "$1" "$2"
      options['lang']="$2"
      shift 2
      ;;
    --option*)            # switch - Options to set
      check_value "$1" "$2"
      options_list+=("$2")
      shift 2
      ;;
    --preserve*|--leave*)   # switch - Preserve item from file (e.g. track)
      check_value "$1" "$2"
      options['preserve']="$2"
      actions_list+=("preserve")
      shift 2
      ;;
    --recursive)          # switch - Enable recursive mode
      options['recursive']="true"
      shift
      ;;
    --set)                # switch - Set information about file
      check_value "$1" "$2"
      options['set']="$2"
      actions_list+=("set")
      shift 2
      ;;
    --shellcheck)         # switch - Run shellcheck against script
      actions_list+=("shellcheck")
      shift
      ;;
    --strict)             # switch - Enable strict mode
      options['strict']="true"
      shift
      ;;
    --swap)               # switch - Swap information about file
      check_value "$1" "$2"
      options['swap']="$2"
      actions_list+=("swap")
      shift 2
      ;;
    --track)              # switch - Track to perform operation on
      check_value "$1" "$2"
      options['track']="$2"
      shift 2
      ;;
    --usage)              # switch - Display usage
      check_value "$1" "$2"
      usage="$2"
      print_usage "${usage}"
      shift 2
      exit
      ;;
    --verbose)            # switch - Enable verbose mode
      options['verbose']="true"
      shift
      ;;
    --version|-V)         # switch - Print version information
      print_version
      exit
      ;;
    *)
      print_help
      shift
      exit
      ;;
  esac
done

# Process options

if [ -n "${options_list[*]}" ]; then
  for list in "${options_list[@]}"; do
    if [[ "${list}" =~ "," ]]; then
      IFS="," read -r -a array <<< "${list[*]}"
      for item in "${array[@]}"; do
        process_options "${item}"
      done
    else
      process_options "${list}"
    fi
  done
fi

# Reset defaults based on switches

reset_defaults

# Process actions

if [ -n "${actions_list[*]}" ]; then
  for list in "${actions_list[@]}"; do
    if [[ "${list}" =~ "," ]]; then
      IFS="," read -r -a array <<< "${list[*]}"
      for item in "${array[@]}"; do
        process_actions "${item}"
      done
    else
      process_actions "${list}"
    fi
  done
fi
