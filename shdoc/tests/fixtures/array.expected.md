This file contains functions for manipulating arrays

## Index

* [array::contains](#arraycontains)
* [array::join](#arrayjoin)
* [array::nth](#arraynth)

### array::contains

Check if an array contains a value

#### Example

```bash
local -a arr=("a" "b" "c" "d" "e")
array::contains "a" "${arr[@]}" # succeeds (returns 0)
array::contains "z" "${arr[@]}" # fails (returns 1)
```

#### Arguments

* **$1** (string): needle
* **...** (array): elements to search

#### Exit codes

* **0**: If the array contains the needle
* **1**: If the array does not contain the needle

### array::join

Join an array with a delimiter

#### Example

```bash
local -a arr=("a" "b" "c" "d" "e")
array::join "," "${arr[@]}" # a,b,c,d,e
```

#### Arguments

* **$1** (string): delimiter
* **...** (array): elements to join

#### Output on stdout

* The joined array

### array::nth

Get the nth element of an array

#### Example

```bash
local -a new_arr arr=("a" "b" "c" "d" "e")
array::nth new_arr 2 "${arr[@]}"
echo "${new_arr[@]}" # b d
```

#### Arguments

* **$1** (string): new array name
* **$2** (integer): nth element
* **...** (array): elements to get nth element from

